import type { Ticket } from "../../types/ticket"

export type FilterMatch = "all" | "any"
export type FilterField =
  "status" | "priority" | "type" | "assignee" | "labels" | "blocked" | "relation" | "created"
export type FilterOperator = "is" | "is_not" | "includes" | "excludes" | "before" | "after"

export type FilterCondition = {
  id: string
  field: FilterField
  operator: FilterOperator
  value: string
}

export type FilterGroup = {
  id?: string
  match: FilterMatch
  conditions: FilterCondition[]
  groups?: FilterGroup[]
  query?: string
}
export type FilterState = FilterGroup

export const emptyFilters: FilterState = { match: "all", conditions: [], groups: [], query: "" }

export const fieldLabels: Record<FilterField, string> = {
  status: "Status",
  priority: "Priority",
  type: "Type",
  assignee: "Assignee",
  labels: "Label",
  blocked: "Blocked",
  relation: "Relationship",
  created: "Created",
}

export function operatorsFor(field: FilterField): Array<{ value: FilterOperator; label: string }> {
  if (field === "labels")
    return [
      { value: "includes", label: "includes" },
      { value: "excludes", label: "excludes" },
    ]
  if (field === "created")
    return [
      { value: "after", label: "is after" },
      { value: "before", label: "is before" },
    ]
  return [
    { value: "is", label: "is" },
    { value: "is_not", label: "is not" },
  ]
}

export function defaultCondition(field: FilterField = "status"): FilterCondition {
  const defaults: Record<FilterField, string> = {
    status: "open",
    priority: "2",
    type: "task",
    assignee: "unassigned",
    labels: "",
    blocked: "true",
    relation: "top_level",
    created: new Date().toISOString().slice(0, 10),
  }
  return {
    id: crypto.randomUUID(),
    field,
    operator: operatorsFor(field)[0].value,
    value: defaults[field],
  }
}

export function ticketMatchesFilters(
  ticket: Ticket,
  allTickets: Ticket[],
  filters: FilterState,
  currentUser?: string,
): boolean {
  return (
    matchesGroup(ticket, allTickets, filters) &&
    ticketMatchesQuery(ticket, allTickets, filters.query ?? "", currentUser)
  )
}

type QueryNode = { kind: "term"; value: string } | { kind: "any"; values: string[] }

export function ticketMatchesQuery(
  ticket: Ticket,
  allTickets: Ticket[],
  query: string,
  currentUser?: string,
): boolean {
  const tokens = tokenizeQuery(query)
  if (tokens.length === 0) return true
  const clauses: QueryNode[][] = [[]]
  for (let index = 0; index < tokens.length; index++) {
    const token = tokens[index]
    if (token === "OR") {
      clauses.push([])
      continue
    }
    if (token === "{") {
      const values: string[] = []
      while (++index < tokens.length && tokens[index] !== "}")
        if (tokens[index] !== "OR") values.push(tokens[index])
      if (values.length) clauses.at(-1)!.push({ kind: "any", values })
      continue
    }
    if (token !== "}") clauses.at(-1)!.push({ kind: "term", value: token })
  }
  return clauses.some(
    (clause) =>
      clause.length > 0 &&
      clause.every((node) =>
        node.kind === "term"
          ? matchesQueryTerm(ticket, allTickets, node.value, currentUser)
          : node.values.some((value) => matchesQueryTerm(ticket, allTickets, value, currentUser)),
      ),
  )
}

export function tokenizeQuery(query: string): string[] {
  const tokens: string[] = []
  let current = "",
    quote = "",
    escaped = false
  const flush = () => {
    if (current) tokens.push(current)
    current = ""
  }
  for (const character of query.trim()) {
    if (escaped) {
      current += character
      escaped = false
      continue
    }
    if (character === "\\" && quote) {
      escaped = true
      continue
    }
    if (quote) {
      if (character === quote) quote = ""
      else current += character
      continue
    }
    if (character === '"' || character === "'") {
      quote = character
      continue
    }
    if (character === "{" || character === "}") {
      flush()
      tokens.push(character)
      continue
    }
    if (/\s/.test(character)) {
      flush()
      continue
    }
    current += character
  }
  if (escaped) current += "\\"
  flush()
  return tokens
}

function matchesQueryTerm(
  ticket: Ticket,
  allTickets: Ticket[],
  raw: string,
  currentUser?: string,
): boolean {
  const negated = raw.startsWith("-") && raw.length > 1
  const token = negated ? raw.slice(1) : raw
  const separator = token.indexOf(":")
  const field = separator > 0 ? token.slice(0, separator).toLowerCase() : ""
  const value = separator > 0 ? token.slice(separator + 1).toLowerCase() : token.toLowerCase()
  const known = [
    "status",
    "priority",
    "type",
    "assignee",
    "label",
    "labels",
    "tag",
    "parent",
    "id",
    "title",
    "blocked",
    "before",
    "after",
    "created",
    "is",
    "has",
  ]
  const text = [
    ticket.id,
    ticket.title,
    ticket.description,
    ticket.assignee,
    ticket.type,
    ...ticket.tags,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase()
  let matches = false
  if (!field || !known.includes(field)) matches = text.includes(token.toLowerCase())
  else if (!value) matches = false
  else if (field === "status") matches = ticket.status === normalizeStatus(value)
  else if (field === "priority") {
    const rawPriority = value.replace(/^p/, "")
    matches = /^\d+$/.test(rawPriority) && ticket.priority === Number(rawPriority)
  } else if (field === "type") matches = matchValue(ticket.type, value)
  else if (field === "assignee")
    matches =
      value === "unassigned" || value === "none"
        ? !ticket.assignee
        : value === "me"
          ? Boolean(currentUser && ticket.assignee?.toLowerCase() === currentUser.toLowerCase())
          : matchValue(ticket.assignee ?? "", value)
  else if (["label", "labels", "tag"].includes(field))
    matches = ticket.tags.some((tag) => matchValue(tag, value))
  else if (field === "parent")
    matches = ["none", "top-level", "top_level"].includes(value)
      ? !ticket.parent
      : matchValue(ticket.parent ?? "", value)
  else if (field === "id") matches = matchValue(ticket.id, value)
  else if (field === "title") matches = matchValue(ticket.title, value)
  else if (field === "blocked") {
    const expected = parseBoolean(value)
    matches = expected !== undefined && ticket.blocked === expected
  } else if (field === "before" || field === "after" || field === "created") {
    const created = ticket.created.slice(0, 10)
    matches =
      validDate(created) &&
      validDate(value) &&
      (field === "before"
        ? created < value
        : field === "after"
          ? created >= value
          : created === value)
  } else if (field === "is") matches = matchesIs(ticket, allTickets, value)
  else if (field === "has") matches = matchesHas(ticket, allTickets, value)
  return negated ? !matches : matches
}

function normalizeStatus(value: string): Ticket["status"] | string {
  if (["todo", "open"].includes(value)) return "open"
  if (["started", "active", "in-progress", "in_progress"].includes(value)) return "in_progress"
  if (["done", "closed"].includes(value)) return "closed"
  return value
}

function matchesIs(ticket: Ticket, allTickets: Ticket[], value: string): boolean {
  if (
    ["open", "todo", "active", "started", "in-progress", "in_progress", "closed", "done"].includes(
      value,
    )
  )
    return ticket.status === normalizeStatus(value)
  if (value === "blocked") return ticket.blocked
  if (value === "unblocked") return !ticket.blocked
  if (value === "ready") return ticket.status !== "closed" && !ticket.blocked
  if (value === "assigned") return Boolean(ticket.assignee)
  if (value === "unassigned") return !ticket.assignee
  if (["subticket", "sub-ticket", "child"].includes(value)) return Boolean(ticket.parent)
  if (["top-level", "top_level", "root"].includes(value)) return !ticket.parent
  if (value === "parent") return allTickets.some((candidate) => candidate.parent === ticket.id)
  return false
}

function matchesHas(ticket: Ticket, allTickets: Ticket[], value: string): boolean {
  if (value === "parent") return Boolean(ticket.parent)
  if (["children", "subtickets", "sub-tickets"].includes(value))
    return allTickets.some((candidate) => candidate.parent === ticket.id)
  if (["deps", "dependencies", "blockers"].includes(value)) return ticket.deps.length > 0
  if (["links", "related"].includes(value)) return ticket.links.length > 0
  if (value === "assignee") return Boolean(ticket.assignee)
  if (["label", "labels", "tags"].includes(value)) return ticket.tags.length > 0
  if (value === "description") return Boolean(ticket.description.trim())
  if (value === "notes") return ticket.notes.length > 0
  if (["external-ref", "external_ref"].includes(value)) return Boolean(ticket.external_ref)
  if (value === "design") return ticket.raw.includes("\n## Design")
  if (value === "acceptance") return ticket.raw.includes("\n## Acceptance Criteria")
  return false
}

function parseBoolean(value: string): boolean | undefined {
  if (["true", "yes", "1"].includes(value)) return true
  if (["false", "no", "0"].includes(value)) return false
  return undefined
}

function validDate(value: string): boolean {
  const parsed = new Date(value + "T00:00:00Z")
  return (
    /^\d{4}-\d{2}-\d{2}$/.test(value) &&
    !Number.isNaN(parsed.getTime()) &&
    parsed.toISOString().slice(0, 10) === value
  )
}

function matchValue(actual: string, expected: string): boolean {
  const value = actual.toLowerCase()
  if (!expected.includes("*")) return value.includes(expected)
  const pattern = expected
    .split("*")
    .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join(".*")
  return new RegExp(`^${pattern}$`, "i").test(actual)
}

function matchesGroup(ticket: Ticket, allTickets: Ticket[], filters: FilterGroup): boolean {
  const results = [
    ...filters.conditions.map((condition) => matchesCondition(ticket, allTickets, condition)),
    ...(filters.groups ?? []).map((group) => matchesGroup(ticket, allTickets, group)),
  ]
  if (results.length === 0) return true
  return filters.match === "all" ? results.every(Boolean) : results.some(Boolean)
}

function matchesCondition(
  ticket: Ticket,
  allTickets: Ticket[],
  condition: FilterCondition,
): boolean {
  let matches = false
  switch (condition.field) {
    case "status":
      matches = ticket.status === condition.value
      break
    case "priority":
      matches = ticket.priority === Number(condition.value)
      break
    case "type":
      matches = ticket.type === condition.value
      break
    case "assignee":
      matches =
        condition.value === "unassigned" ? !ticket.assignee : ticket.assignee === condition.value
      break
    case "labels":
      matches = ticket.tags.some((tag) => tag.toLowerCase() === condition.value.toLowerCase())
      break
    case "blocked":
      matches = ticket.blocked === (condition.value === "true")
      break
    case "relation": {
      const hasChildren = allTickets.some((candidate) => candidate.parent === ticket.id)
      matches =
        condition.value === "top_level"
          ? !ticket.parent
          : condition.value === "sub_ticket"
            ? Boolean(ticket.parent)
            : hasChildren
      break
    }
    case "created": {
      const created = new Date(ticket.created).getTime()
      const selected = new Date(condition.value + "T00:00:00").getTime()
      matches =
        Number.isFinite(created) &&
        Number.isFinite(selected) &&
        (condition.operator === "before" ? created < selected : created >= selected)
      return matches
    }
  }
  return condition.operator === "is_not" || condition.operator === "excludes" ? !matches : matches
}
