use std::{path::Path, process::Stdio, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{process::Command, time::timeout};

#[derive(Debug, Serialize)]
pub struct ReferenceResponse {
    pub available: bool,
    pub repository: Option<Repository>,
    pub items: Vec<ReferenceItem>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Repository {
    pub name_with_owner: String,
    pub url: String,
    pub host: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReferenceItem {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub kind: String,
}

#[derive(Deserialize)]
struct RepoOutput {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
    url: String,
}

#[derive(Deserialize)]
struct GhItem {
    number: u64,
    title: String,
    url: String,
    state: String,
}

pub async fn references(workspace: &Path, query: &str) -> ReferenceResponse {
    let repository = match repository(workspace).await {
        Ok(repository) => repository,
        Err(reason) => {
            return ReferenceResponse {
                available: false,
                repository: None,
                items: Vec::new(),
                reason: Some(reason),
            };
        }
    };
    let spec = repository_spec(&repository);
    let issue = list(workspace, "issue", &spec, query);
    let pull = list(workspace, "pr", &spec, query);
    let (issues, pulls) = tokio::join!(issue, pull);
    let (mut items, reason) = match (issues, pulls) {
        (Ok(mut issues), Ok(pulls)) => {
            issues.extend(pulls);
            (issues, None)
        }
        (Ok(issues), Err(reason)) | (Err(reason), Ok(issues)) => (issues, Some(reason)),
        (Err(issue), Err(pull)) => {
            return ReferenceResponse {
                available: false,
                repository: Some(repository),
                items: Vec::new(),
                reason: Some(format!(
                    "GitHub issue lookup failed: {issue}; PR lookup failed: {pull}"
                )),
            };
        }
    };
    items.sort_by(|a, b| b.number.cmp(&a.number).then_with(|| a.kind.cmp(&b.kind)));
    items.truncate(16);
    ReferenceResponse {
        available: true,
        repository: Some(repository),
        items,
        reason,
    }
}

fn repository_spec(repository: &Repository) -> String {
    if repository.host == "github.com" {
        repository.name_with_owner.clone()
    } else {
        format!("{}/{}", repository.host, repository.name_with_owner)
    }
}

async fn repository(workspace: &Path) -> Result<Repository, String> {
    let output = gh(workspace, &["repo", "view", "--json", "nameWithOwner,url"]).await?;
    let parsed: RepoOutput = serde_json::from_slice(&output)
        .map_err(|error| format!("gh returned invalid repository data: {error}"))?;
    let host = repository_host(&parsed.url)
        .ok_or_else(|| format!("cannot determine GitHub host from {}", parsed.url))?;
    Ok(Repository {
        name_with_owner: parsed.name_with_owner,
        url: parsed.url,
        host,
    })
}

async fn list(
    workspace: &Path,
    kind: &str,
    repository: &str,
    query: &str,
) -> Result<Vec<ReferenceItem>, String> {
    let mut arguments = vec![
        kind,
        "list",
        "--repo",
        repository,
        "--state",
        "all",
        "--limit",
        "12",
        "--json",
        "number,title,url,state",
    ];
    if !query.trim().is_empty() {
        arguments.extend(["--search", query.trim()]);
    }
    let output = gh(workspace, &arguments).await?;
    let values: Vec<GhItem> = serde_json::from_slice(&output)
        .map_err(|error| format!("gh returned invalid {kind} data: {error}"))?;
    Ok(values
        .into_iter()
        .map(|item| ReferenceItem {
            number: item.number,
            title: item.title,
            url: item.url,
            state: item.state.to_lowercase(),
            kind: if kind == "pr" {
                "pull_request"
            } else {
                "issue"
            }
            .into(),
        })
        .collect())
}

async fn gh(workspace: &Path, arguments: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new("gh");
    command
        .args(arguments)
        .current_dir(workspace)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env_remove("GH_REPO")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = timeout(Duration::from_secs(8), command.output())
        .await
        .map_err(|_| "GitHub lookup timed out".to_owned())?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "GitHub CLI (gh) is not installed".to_owned()
            } else {
                format!("cannot run gh: {error}")
            }
        })?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let reason = String::from_utf8_lossy(&output.stderr)
        .lines()
        .next()
        .unwrap_or("GitHub CLI request failed")
        .trim()
        .to_owned();
    Err(if reason.is_empty() {
        "GitHub CLI request failed".into()
    } else {
        reason
    })
}

fn repository_host(url: &str) -> Option<String> {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    without_scheme
        .split('/')
        .next()
        .filter(|host| !host.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_public_and_enterprise_hosts() {
        assert_eq!(
            repository_host("https://github.com/openai/codex"),
            Some("github.com".into())
        );
        assert_eq!(
            repository_host("https://github.example.com/acme/app"),
            Some("github.example.com".into())
        );
        assert_eq!(
            repository_spec(&Repository {
                name_with_owner: "acme/app".into(),
                url: "https://github.com/acme/app".into(),
                host: "github.com".into(),
            }),
            "acme/app"
        );
        assert_eq!(
            repository_spec(&Repository {
                name_with_owner: "acme/app".into(),
                url: "https://github.example.com/acme/app".into(),
                host: "github.example.com".into(),
            }),
            "github.example.com/acme/app"
        );
    }

    #[tokio::test]
    async fn missing_repository_is_a_nonfatal_response() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-gh-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(&root).await.unwrap();

        let response = references(&root, "ticket").await;

        assert!(!response.available);
        assert!(response.items.is_empty());
        assert!(response.reason.is_some());
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
