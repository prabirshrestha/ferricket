use std::collections::HashMap;

use chrono::NaiveDate;

use crate::storage::{self, Ticket};

#[derive(Debug, PartialEq, Eq)]
enum QueryNode {
    Term(String),
    Any(Vec<String>),
}

/// Match a ticket using Ferricket's Gmail-style query language.
///
/// Space-separated terms use implicit AND, OR separates alternatives, braces
/// form an OR group, and a leading minus negates one term. Quoted values stay
/// in one token. Unknown prefixes intentionally fall back to free-text search.
pub fn matches(ticket: &Ticket, all: &[Ticket], query: &str, current_user: Option<&str>) -> bool {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return true;
    }

    let mut clauses = vec![Vec::new()];
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token == "OR" {
            clauses.push(Vec::new());
        } else if token == "{" {
            let mut values = Vec::new();
            index += 1;
            while index < tokens.len() && tokens[index] != "}" {
                if tokens[index] != "OR" {
                    values.push(tokens[index].clone());
                }
                index += 1;
            }
            if !values.is_empty() {
                clauses
                    .last_mut()
                    .expect("query always has a clause")
                    .push(QueryNode::Any(values));
            }
        } else if token != "}" {
            clauses
                .last_mut()
                .expect("query always has a clause")
                .push(QueryNode::Term(token.clone()));
        }
        index += 1;
    }

    let map = storage::ticket_map(all);
    clauses.iter().any(|clause| {
        !clause.is_empty()
            && clause.iter().all(|node| match node {
                QueryNode::Term(value) => matches_term(ticket, all, &map, value, current_user),
                QueryNode::Any(values) => values
                    .iter()
                    .any(|value| matches_term(ticket, all, &map, value, current_user)),
            })
    })
}

pub fn tokenize(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for character in query.trim().chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if matches!(character, '{' | '}') {
            flush(&mut current, &mut tokens);
            tokens.push(character.to_string());
        } else if character.is_whitespace() {
            flush(&mut current, &mut tokens);
        } else {
            current.push(character);
        }
    }
    if escaped {
        current.push('\\');
    }
    flush(&mut current, &mut tokens);
    tokens
}

fn flush(current: &mut String, tokens: &mut Vec<String>) {
    if !current.is_empty() {
        tokens.push(std::mem::take(current));
    }
}

fn matches_term(
    ticket: &Ticket,
    all: &[Ticket],
    map: &HashMap<String, Ticket>,
    raw: &str,
    current_user: Option<&str>,
) -> bool {
    let (negated, token) = raw
        .strip_prefix('-')
        .filter(|value| !value.is_empty())
        .map_or((false, raw), |value| (true, value));
    let (field, value) = token
        .split_once(':')
        .filter(|(field, _)| !field.is_empty())
        .map_or(("", token), |(field, value)| (field, value));
    let field = field.to_ascii_lowercase();
    let value = value.to_lowercase();

    let matched = if is_known_field(&field) && value.is_empty() {
        false
    } else {
        match field.as_str() {
            "" => free_text(ticket).contains(&token.to_lowercase()),
            "status" => ticket
                .status()
                .eq_ignore_ascii_case(normalize_status(&value)),
            "priority" => value
                .strip_prefix('p')
                .unwrap_or(&value)
                .parse::<u8>()
                .is_ok_and(|priority| ticket.priority() == priority),
            "type" => match_value(ticket.field("type"), &value),
            "assignee" => {
                if matches!(value.as_str(), "unassigned" | "none") {
                    ticket.field("assignee").is_empty()
                } else if value == "me" {
                    current_user
                        .is_some_and(|user| ticket.field("assignee").eq_ignore_ascii_case(user))
                } else {
                    match_value(ticket.field("assignee"), &value)
                }
            }
            "label" | "labels" | "tag" => ticket
                .array("tags")
                .iter()
                .any(|tag| match_value(tag, &value)),
            "parent" => {
                if matches!(value.as_str(), "none" | "top-level" | "top_level") {
                    ticket.field("parent").is_empty()
                } else {
                    match_value(ticket.field("parent"), &value)
                }
            }
            "id" => match_value(ticket.id(), &value),
            "title" => match_value(&ticket.title, &value),
            "blocked" => {
                parse_bool(&value).is_some_and(|expected| is_blocked(ticket, map) == expected)
            }
            "before" | "after" | "created" => matches_date(ticket, &field, &value),
            "is" => matches_is(ticket, all, map, &value),
            "has" => matches_has(ticket, all, &value),
            _ => free_text(ticket).contains(&token.to_lowercase()),
        }
    };
    if negated { !matched } else { matched }
}

pub async fn current_user() -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
        .await
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn free_text(ticket: &Ticket) -> String {
    [
        ticket.id().to_owned(),
        ticket.title.clone(),
        ticket.description(),
        ticket.array("tags").join(" "),
        ticket.field("assignee").to_owned(),
        ticket.field("type").to_owned(),
    ]
    .join(" ")
    .to_lowercase()
}

fn normalize_status(value: &str) -> &str {
    match value {
        "todo" | "open" => "open",
        "started" | "active" | "in-progress" | "in_progress" => "in_progress",
        "done" | "closed" => "closed",
        other => other,
    }
}

fn is_known_field(field: &str) -> bool {
    matches!(
        field,
        "status"
            | "priority"
            | "type"
            | "assignee"
            | "label"
            | "labels"
            | "tag"
            | "parent"
            | "id"
            | "title"
            | "blocked"
            | "before"
            | "after"
            | "created"
            | "is"
            | "has"
    )
}

fn matches_is(ticket: &Ticket, all: &[Ticket], map: &HashMap<String, Ticket>, value: &str) -> bool {
    match value {
        "open" | "todo" | "active" | "started" | "in-progress" | "in_progress" | "closed"
        | "done" => ticket.status() == normalize_status(value),
        "blocked" => is_blocked(ticket, map),
        "unblocked" => !is_blocked(ticket, map),
        "ready" => ticket.status() != "closed" && !is_blocked(ticket, map),
        "assigned" => !ticket.field("assignee").is_empty(),
        "unassigned" => ticket.field("assignee").is_empty(),
        "subticket" | "sub-ticket" | "child" => !ticket.field("parent").is_empty(),
        "top-level" | "top_level" | "root" => ticket.field("parent").is_empty(),
        "parent" => all
            .iter()
            .any(|candidate| candidate.field("parent") == ticket.id()),
        _ => false,
    }
}

fn matches_has(ticket: &Ticket, all: &[Ticket], value: &str) -> bool {
    match value {
        "parent" => !ticket.field("parent").is_empty(),
        "children" | "subtickets" | "sub-tickets" => all
            .iter()
            .any(|candidate| candidate.field("parent") == ticket.id()),
        "deps" | "dependencies" | "blockers" => !ticket.array("deps").is_empty(),
        "links" | "related" => !ticket.array("links").is_empty(),
        "assignee" => !ticket.field("assignee").is_empty(),
        "label" | "labels" | "tags" => !ticket.array("tags").is_empty(),
        "description" => !ticket.description().trim().is_empty(),
        "notes" => !ticket.notes().is_empty(),
        "external-ref" | "external_ref" => !ticket.field("external-ref").is_empty(),
        "design" => ticket.raw.contains("\n## Design"),
        "acceptance" => ticket.raw.contains("\n## Acceptance Criteria"),
        _ => false,
    }
}

fn is_blocked(ticket: &Ticket, map: &HashMap<String, Ticket>) -> bool {
    !storage::unresolved(ticket, map).is_empty()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn matches_date(ticket: &Ticket, field: &str, expected: &str) -> bool {
    let Some(created) = ticket.field("created").get(..10) else {
        return false;
    };
    if NaiveDate::parse_from_str(created, "%Y-%m-%d").is_err()
        || NaiveDate::parse_from_str(expected, "%Y-%m-%d").is_err()
    {
        return false;
    }
    match field {
        "before" => created < expected,
        "after" => created >= expected,
        _ => created == expected,
    }
}

fn match_value(actual: &str, expected: &str) -> bool {
    let actual = actual.to_lowercase();
    if !expected.contains('*') {
        return actual.contains(expected);
    }
    wildcard_match(&actual, expected)
}

fn wildcard_match(actual: &str, pattern: &str) -> bool {
    let actual = actual.chars().collect::<Vec<_>>();
    let pattern = pattern.chars().collect::<Vec<_>>();
    let mut previous = vec![false; actual.len() + 1];
    previous[0] = true;
    for character in pattern {
        let mut next = vec![false; actual.len() + 1];
        if character == '*' {
            next[0] = previous[0];
            for index in 1..=actual.len() {
                next[index] = previous[index] || next[index - 1];
            }
        } else {
            for index in 1..=actual.len() {
                next[index] = previous[index - 1] && actual[index - 1] == character;
            }
        }
        previous = next;
    }
    previous[actual.len()]
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf, time::SystemTime};

    use super::*;

    fn ticket(id: &str, title: &str) -> Ticket {
        Ticket {
            path: PathBuf::from(format!("{id}.md")),
            raw: format!("---\nid: {id}\n---\n# {title}\n\nSearch owner:platform here.\n"),
            fields: BTreeMap::from([
                ("id".into(), id.into()),
                ("status".into(), "open".into()),
                ("priority".into(), "1".into()),
                ("type".into(), "feature".into()),
                ("assignee".into(), "Ada".into()),
                ("tags".into(), "[frontend, ux]".into()),
                ("deps".into(), "[]".into()),
                ("links".into(), "[]".into()),
                ("created".into(), "2026-07-10T00:00:00Z".into()),
            ]),
            title: title.into(),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn tokenizes_quotes_braces_negation_and_escaped_quotes() {
        assert_eq!(
            tokenize(
                r#"status:open title:"Build \"great\" filters" {label:frontend label:rust} -is:blocked"#
            ),
            [
                "status:open",
                "title:Build \"great\" filters",
                "{",
                "label:frontend",
                "label:rust",
                "}",
                "-is:blocked",
            ]
        );
    }

    #[test]
    fn supports_and_or_negation_aliases_groups_and_unknown_fields() {
        let mut parent = ticket("fer-main", "Build filters");
        parent.fields.insert("deps".into(), "[fer-blocker]".into());
        let mut child = ticket("fer-child", "Write docs");
        child.fields.insert("parent".into(), parent.id().into());
        child.fields.insert("assignee".into(), String::new());
        child.fields.insert("tags".into(), "[docs]".into());
        let all = vec![parent.clone(), child];

        assert!(matches(
            &parent,
            &all,
            "status:todo label:frontend -assignee:unassigned",
            None,
        ));
        assert!(matches(
            &parent,
            &all,
            r#"id:nope OR title:"Build filters""#,
            None,
        ));
        assert!(matches(
            &parent,
            &all,
            "{label:backend label:frontend} is:blocked",
            None,
        ));
        assert!(matches(
            &parent,
            &all,
            "has:children after:2026-07-01",
            None
        ));
        assert!(matches(&parent, &all, "owner:platform", None));
        let word = ticket("fer-or", "Use or as ordinary text");
        assert!(matches(&word, std::slice::from_ref(&word), "or", None));
        assert!(matches(&parent, &all, "assignee:me", Some("ada")));
        assert!(!matches(&parent, &all, "assignee:me", None));
        assert!(!matches(&parent, &all, "before:not-a-date", None));
        assert!(!matches(&parent, &all, "title:", None));
    }

    #[test]
    fn wildcard_values_are_case_insensitive_and_anchored() {
        let ticket = ticket("fer-main", "Build filters");
        assert!(matches(
            &ticket,
            std::slice::from_ref(&ticket),
            "title:build*",
            None,
        ));
        assert!(!matches(
            &ticket,
            std::slice::from_ref(&ticket),
            "title:filters*",
            None,
        ));
    }
}
