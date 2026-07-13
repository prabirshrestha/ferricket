import type { Ticket } from "../../types/ticket"

export type QuerySuggestion = {
  id: string
  label: string
  detail: string
  replacement: string
  complete?: boolean
  keywords?: string
}
export type TokenRange = { start: number; end: number; token: string }

const fields: QuerySuggestion[] = [
  ["status", "Ticket status"],
  ["priority", "Priority P0–P4"],
  ["type", "Ticket type"],
  ["assignee", "Assignee"],
  ["label", "Label or tag"],
  ["tag", "Label or tag alias"],
  ["parent", "Parent ticket"],
  ["id", "Ticket ID"],
  ["title", "Title text"],
  ["blocked", "Blocked state"],
  ["before", "Created before date"],
  ["after", "Created on or after date"],
  ["created", "Exact created date"],
  ["is", "State predicate"],
  ["has", "Presence predicate"],
].map(([field, detail]) => ({
  id: "field:" + field,
  label: field + ":",
  detail,
  replacement: field + ":",
}))

export function tokenAt(query: string, cursor = query.length): TokenRange {
  const caret = Math.max(0, Math.min(cursor, query.length))
  let start: number | null = null
  let quote = ""
  let escaped = false
  const ranges: Array<{ start: number; end: number }> = []
  for (let index = 0; index <= query.length; index++) {
    const character = query[index] ?? " "
    if (start === null) {
      if (!/\s|\{|\}/.test(character)) {
        start = index
        if (character === '"' || character === "'") quote = character
      }
      continue
    }
    if (escaped) {
      escaped = false
      continue
    }
    if (quote) {
      if (character === "\\") escaped = true
      else if (character === quote) quote = ""
      continue
    }
    if (character === '"' || character === "'") quote = character
    else if (/\s|\{|\}/.test(character)) {
      ranges.push({ start, end: index })
      start = null
    }
  }
  if (start !== null && !ranges.some((item) => item.start === start))
    ranges.push({ start, end: query.length })
  const range = ranges.find((item) => caret >= item.start && caret <= item.end)
  return range
    ? { ...range, token: query.slice(range.start, range.end) }
    : { start: caret, end: caret, token: "" }
}

export function querySuggestions(
  query: string,
  tickets: Ticket[],
  cursor = query.length,
): QuerySuggestion[] {
  const { token } = tokenAt(query, cursor)
  const negated = token.startsWith("-")
  const bare = negated ? token.slice(1) : token
  const [field, typed = ""] = bare.includes(":") ? bare.split(/:(.*)/s, 2) : ["", bare]
  if (!field) {
    const operators: QuerySuggestion[] = [
      {
        id: "operator:OR",
        label: "OR",
        detail: "Match either side",
        replacement: "OR",
        complete: true,
      },
      {
        id: "group:open",
        label: "{ … }",
        detail: "Match any term in a group",
        replacement: "{",
        complete: true,
      },
    ]
    return [...fields, ...operators]
      .filter((item) => matches(item, typed))
      .slice(0, 14)
      .map((item) =>
        negated && item.id.startsWith("field:")
          ? { ...item, replacement: "-" + item.replacement }
          : item,
      )
  }
  const values = valuesFor(field.toLowerCase(), tickets)
  return values
    .filter((item) => matches(item, typed))
    .slice(0, 18)
    .map((item) => ({
      ...item,
      replacement: (negated ? "-" : "") + field + ":" + item.replacement,
    }))
}

export function applySuggestion(
  query: string,
  suggestion: QuerySuggestion,
  cursor = query.length,
): { value: string; cursor: number } {
  const range = tokenAt(query, cursor)
  const replacement = suggestion.replacement + (suggestion.complete ? " " : "")
  let suffixStart = range.end
  if (suggestion.complete) while (/\s/.test(query[suffixStart] ?? "")) suffixStart += 1
  return {
    value: query.slice(0, range.start) + replacement + query.slice(suffixStart),
    cursor: range.start + replacement.length,
  }
}

function valuesFor(field: string, tickets: Ticket[]): QuerySuggestion[] {
  const values = (entries: Array<[string, string]>) =>
    entries.map(([value, detail]) => ({
      id: field + ":" + value,
      label: value,
      detail,
      replacement: value,
      complete: true,
    }))
  if (field === "status")
    return values([
      ["todo", "Open tickets"],
      ["active", "In progress"],
      ["done", "Closed tickets"],
    ])
  if (field === "priority")
    return values(
      [0, 1, 2, 3, 4].map((value) => [
        "p" + value,
        value === 0
          ? "Urgent"
          : value === 1
            ? "High"
            : value === 2
              ? "Medium"
              : value === 3
                ? "Low"
                : "No priority",
      ]),
    )
  if (field === "blocked")
    return values([
      ["true", "Blocked"],
      ["false", "Not blocked"],
    ])
  if (field === "is")
    return values(
      [
        "blocked",
        "unblocked",
        "ready",
        "assigned",
        "unassigned",
        "subticket",
        "top-level",
        "parent",
      ].map((value) => [value, "State predicate"]),
    )
  if (field === "has")
    return values(
      [
        "children",
        "deps",
        "links",
        "labels",
        "description",
        "notes",
        "assignee",
        "parent",
        "design",
        "acceptance",
        "external-ref",
      ].map((value) => [value, "Presence predicate"]),
    )
  if (field === "assignee")
    return values([
      ["me", "Current Git user"],
      ["unassigned", "No assignee"],
      ...unique(tickets.flatMap((ticket) => (ticket.assignee ? [ticket.assignee] : []))).map(
        (value) => [quote(value), "Workspace assignee"] as [string, string],
      ),
    ])
  if (["label", "labels", "tag"].includes(field))
    return values(
      unique(tickets.flatMap((ticket) => ticket.tags)).map((value) => [
        quote(value),
        "Workspace label",
      ]),
    )
  if (field === "type")
    return values(
      unique(tickets.map((ticket) => ticket.type)).map((value) => [quote(value), "Ticket type"]),
    )
  if (field === "parent")
    return values([
      ["none", "No parent"],
      ...tickets.map((ticket) => [ticket.id, ticket.title] as [string, string]),
    ])
  if (field === "id") return values(tickets.map((ticket) => [ticket.id, ticket.title]))
  if (field === "title") return values(tickets.map((ticket) => [quote(ticket.title), ticket.id]))
  if (["before", "after", "created"].includes(field))
    return values([[new Date().toISOString().slice(0, 10), "Today · YYYY-MM-DD"]])
  return []
}

function quote(value: string) {
  return /\s/.test(value) ? '"' + value.replaceAll('"', '\\"') + '"' : value
}
function matches(item: QuerySuggestion, query: string) {
  const needle = query.toLowerCase()
  return (
    !needle ||
    (item.label + " " + item.detail + " " + (item.keywords ?? "")).toLowerCase().includes(needle)
  )
}
function unique(values: string[]) {
  return [...new Set(values.filter(Boolean))].sort((a, b) => a.localeCompare(b))
}
