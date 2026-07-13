import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import {
  mergeReferenceSuggestions,
  editorSuggestionKey,
  personSuggestions,
  ticketSuggestions,
  triggerQuery,
} from "./editor-suggestions"

const ticket = (id: string, title: string, assignee?: string): Ticket => ({
  id,
  title,
  assignee,
  revision: "revision",
  path: `.tickets/${id}.md`,
  status: "open",
  deps: [],
  links: [],
  created: "2026-01-01T00:00:00Z",
  updated: "2026-01-02T00:00:00Z",
  type: "task",
  priority: 2,
  tags: [],
  description: "",
  raw: "",
  blocked: false,
  notes: [],
})

test("accepts the highlighted suggestion with Enter or Tab", () => {
  expect(editorSuggestionKey("Enter")).toBe("accept")
  expect(editorSuggestionKey("Tab")).toBe("accept")
  expect(editorSuggestionKey("Escape")).toBe("close")
})

test("finds @ and # tokens only at a word boundary", () => {
  expect(triggerQuery("Hello @ad")).toEqual({ trigger: "@", query: "ad", startOffset: 6 })
  expect(triggerQuery("See #fer")).toEqual({ trigger: "#", query: "fer", startOffset: 4 })
  expect(triggerQuery("hello@example.com")).toBeNull()
})

test("finds unique people and local tickets", () => {
  const tickets = [ticket("fer-1", "Build editor", "Ada"), ticket("fer-2", "Ship CLI", "Ada")]
  expect(personSuggestions(tickets, "Prabir", "a").map((item) => item.label)).toEqual([
    "Ada",
    "Prabir",
  ])
  expect(ticketSuggestions(tickets, "editor").map((item) => item.label)).toEqual(["fer-1"])
})

test("keeps local references before GitHub issues and pull requests", () => {
  const local = ticketSuggestions([ticket("fer-1", "Fix issue")], "")
  const merged = mergeReferenceSuggestions(local, [
    {
      number: 8,
      title: "Fix upstream",
      url: "https://github.com/a/b/issues/8",
      state: "open",
      kind: "issue",
    },
    {
      number: 9,
      title: "Ship fix",
      url: "https://github.com/a/b/pull/9",
      state: "open",
      kind: "pull_request",
    },
  ])
  expect(merged.map((item) => item.kind)).toEqual(["ticket", "issue", "pull_request"])
})
