import type { GitHubReference } from "../../api/tickets"
import type { Ticket } from "../../types/ticket"

export type EditorTrigger = "@" | "#"
export type TriggerMatch = { trigger: EditorTrigger; query: string; startOffset: number }

export type EditorSuggestion = {
  id: string
  kind: "person" | "ticket" | "issue" | "pull_request"
  label: string
  detail: string
  insertText: string
  href?: string
}

export function editorSuggestionKey(key: string) {
  if (key === "ArrowDown") return "next" as const
  if (key === "ArrowUp") return "previous" as const
  if (key === "Enter" || key === "Tab") return "accept" as const
  if (key === "Escape") return "close" as const
  return null
}

export function triggerQuery(textBeforeCaret: string): TriggerMatch | null {
  const match = textBeforeCaret.match(/(?:^|\s)([@#])([^\s@#]*)$/)
  if (!match) return null
  const trigger = match[1] as EditorTrigger
  return {
    trigger,
    query: match[2] ?? "",
    startOffset: textBeforeCaret.lastIndexOf(trigger),
  }
}

export function personSuggestions(
  tickets: Ticket[],
  currentUser: string | undefined,
  query: string,
) {
  const people = [
    ...new Set([currentUser, ...tickets.map((ticket) => ticket.assignee)].filter(Boolean)),
  ]
    .map(String)
    .sort((left, right) => left.localeCompare(right))
  return people
    .filter((person) => matches(person, query))
    .slice(0, 10)
    .map<EditorSuggestion>((person) => ({
      id: `person:${person}`,
      kind: "person",
      label: person,
      detail: person === currentUser ? "You" : "Assignee",
      insertText: `@${person}`,
    }))
}

export function ticketSuggestions(tickets: Ticket[], query: string) {
  return tickets
    .filter((ticket) => matches(`${ticket.id} ${ticket.title}`, query))
    .sort((left, right) => {
      const exactLeft = left.id.toLocaleLowerCase() === query.toLocaleLowerCase() ? 0 : 1
      const exactRight = right.id.toLocaleLowerCase() === query.toLocaleLowerCase() ? 0 : 1
      return (
        exactLeft - exactRight ||
        (right.updated ?? right.created).localeCompare(left.updated ?? left.created)
      )
    })
    .slice(0, 10)
    .map<EditorSuggestion>((ticket) => ({
      id: `ticket:${ticket.id}`,
      kind: "ticket",
      label: ticket.id,
      detail: ticket.title,
      insertText: ticket.id,
      href: `/all/ticket/${encodeURIComponent(ticket.id)}`,
    }))
}

export function mergeReferenceSuggestions(local: EditorSuggestion[], github: GitHubReference[]) {
  const external = github.map<EditorSuggestion>((item) => ({
    id: `github:${item.kind}:${item.number}`,
    kind: item.kind,
    label: `#${item.number}`,
    detail: item.title,
    insertText: `#${item.number}`,
    href: item.url,
  }))
  return [...local, ...external].slice(0, 16)
}

function matches(value: string, query: string) {
  const needle = query.trim().toLocaleLowerCase()
  return !needle || value.toLocaleLowerCase().includes(needle)
}
