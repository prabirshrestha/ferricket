import { describe, expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import {
  ticketMatchesFilters,
  ticketMatchesQuery,
  tokenizeQuery,
  type FilterState,
} from "./filter-types"

const base: Ticket = {
  id: "fer-main",
  revision: "revision-main",
  path: "/tmp/.tickets/fer-main.md",
  status: "open",
  deps: ["fer-blocker"],
  links: [],
  created: "2026-07-10T00:00:00Z",
  updated: "2026-07-10T00:00:00Z",
  type: "feature",
  priority: 1,
  assignee: "Ada",
  tags: ["frontend", "ux"],
  title: "Build filters",
  description: "",
  raw: "",
  blocked: true,
  notes: [],
}
const child: Ticket = {
  ...base,
  id: "fer-child",
  parent: base.id,
  priority: 3,
  assignee: undefined,
  tags: ["docs"],
  blocked: false,
}

describe("ticketMatchesFilters", () => {
  test("combines conditions with all and any semantics", () => {
    const conditions: FilterState["conditions"] = [
      { id: "1", field: "status", operator: "is", value: "open" },
      { id: "2", field: "labels", operator: "includes", value: "frontend" },
      { id: "3", field: "priority", operator: "is", value: "4" },
    ]
    expect(ticketMatchesFilters(base, [base, child], { match: "all", conditions })).toBe(false)
    expect(ticketMatchesFilters(base, [base, child], { match: "any", conditions })).toBe(true)
  })

  test("supports blocked, relationship, assignee, date, and negative conditions", () => {
    const filters: FilterState = {
      match: "all",
      conditions: [
        { id: "1", field: "blocked", operator: "is", value: "true" },
        { id: "2", field: "relation", operator: "is", value: "parent" },
        { id: "3", field: "assignee", operator: "is_not", value: "unassigned" },
        { id: "4", field: "created", operator: "after", value: "2026-07-01" },
        { id: "5", field: "labels", operator: "excludes", value: "backend" },
      ],
    }
    expect(ticketMatchesFilters(base, [base, child], filters)).toBe(true)
    expect(ticketMatchesFilters(child, [base, child], filters)).toBe(false)
  })
})

describe("Gmail-style ticket queries", () => {
  test("tokenizes quoted values, braces, and negation", () => {
    expect(
      tokenizeQuery(
        'status:open assignee:"Ada Lovelace" {label:frontend label:backend} -is:blocked',
      ),
    ).toEqual([
      "status:open",
      "assignee:Ada Lovelace",
      "{",
      "label:frontend",
      "label:backend",
      "}",
      "-is:blocked",
    ])
    expect(tokenizeQuery('title:"ends with\\')).toEqual(["title:ends with\\"])
  })

  test("supports implicit AND, OR, negation, aliases, and brace groups", () => {
    expect(
      ticketMatchesQuery(base, [base, child], "status:todo label:frontend -assignee:unassigned"),
    ).toBe(true)
    expect(
      ticketMatchesQuery(child, [base, child], "status:todo label:frontend -assignee:unassigned"),
    ).toBe(false)
    expect(ticketMatchesQuery(base, [base, child], 'id:nope OR title:"Build filters"')).toBe(true)
    expect(
      ticketMatchesQuery(base, [base, child], "{label:backend label:frontend} is:blocked"),
    ).toBe(true)
    expect(ticketMatchesQuery(base, [base, child], "has:children after:2026-07-01")).toBe(true)
    expect(ticketMatchesQuery({ ...base, title: "Use or as ordinary text" }, [base], "or")).toBe(
      true,
    )
  })

  test("treats unknown prefixes as ordinary free text", () => {
    const withText = { ...base, description: "Investigate owner:platform syntax" }
    expect(ticketMatchesQuery(withText, [withText], "owner:platform")).toBe(true)
  })

  test("rejects malformed typed values rather than matching them accidentally", () => {
    expect(ticketMatchesQuery(base, [base], "blocked:maybe")).toBe(false)
    expect(ticketMatchesQuery(base, [base], "before:not-a-date")).toBe(false)
    expect(ticketMatchesQuery(base, [base], "before:2026-02-31")).toBe(false)
    expect(ticketMatchesQuery(base, [base], "priority:any")).toBe(false)
    expect(ticketMatchesQuery(base, [base], "title:")).toBe(false)
  })

  test("priority parsing matches the Rust query grammar", () => {
    expect(ticketMatchesQuery({ ...base, priority: 0 }, [base], "priority:p")).toBe(false)
    expect(ticketMatchesQuery({ ...base, priority: 2 }, [base], "priority:2.0")).toBe(false)
    expect(ticketMatchesQuery({ ...base, priority: 2 }, [base], "priority:p2")).toBe(true)
  })

  test("supports wildcard values and richer presence aliases", () => {
    const rich = { ...base, raw: "\n## Design\n\nShip it", external_ref: "linear:ENG-42" }
    expect(ticketMatchesQuery(rich, [rich], "title:build* has:design has:external-ref")).toBe(true)
    expect(ticketMatchesQuery(rich, [rich], "is:ready")).toBe(false)
  })

  test("resolves assignee:me against the workspace user", () => {
    expect(ticketMatchesQuery(base, [base], "assignee:me", "ada")).toBe(true)
    expect(ticketMatchesQuery(base, [base], "assignee:me")).toBe(false)
  })
})
