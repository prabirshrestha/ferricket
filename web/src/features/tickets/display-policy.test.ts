import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import { groupTickets, orderTickets, ticketsForView } from "./display-policy"

const tickets = [
  ticket({
    id: "fer-open",
    title: "Beta",
    status: "open",
    priority: 2,
    created: "2026-07-02",
    assignee: "Ada",
  }),
  ticket({
    id: "fer-blocked",
    title: "Alpha",
    status: "in_progress",
    priority: 0,
    created: "2026-07-03",
    blocked: true,
  }),
  ticket({
    id: "fer-closed",
    title: "Done",
    status: "closed",
    priority: 1,
    created: "2026-07-01",
    updated: "2026-07-01",
    assignee: "Ada",
  }),
]

test("view policy gives Show completed real Active-view behavior", () => {
  expect(ticketsForView(tickets, "active", false).map((item) => item.id)).toEqual([
    "fer-open",
    "fer-blocked",
  ])
  expect(ticketsForView(tickets, "active", true).map((item) => item.id)).toEqual([
    "fer-open",
    "fer-blocked",
    "fer-closed",
  ])
  expect(ticketsForView(tickets, "blocked", true).map((item) => item.id)).toEqual(["fer-blocked"])
  expect(ticketsForView(tickets, "closed", false).map((item) => item.id)).toEqual(["fer-closed"])
  expect(ticketsForView(tickets, "all", false)).toHaveLength(3)
})

test("ordering and reverse ordering affect every layout input", () => {
  expect(
    orderTickets(tickets, { orderBy: "priority", descending: false }).map((item) => item.id),
  ).toEqual(["fer-blocked", "fer-closed", "fer-open"])
  expect(
    orderTickets(tickets, { orderBy: "created", descending: true }).map((item) => item.id),
  ).toEqual(["fer-blocked", "fer-open", "fer-closed"])
  expect(
    orderTickets(tickets, { orderBy: "title", descending: false }).map((item) => item.title),
  ).toEqual(["Alpha", "Beta", "Done"])
})

test("grouping produces stable grouped-list sections", () => {
  expect(
    groupTickets(tickets.slice(0, 2), "status").map((group) => [group.label, group.tickets.length]),
  ).toEqual([
    ["open", 1],
    ["in_progress", 1],
    ["closed", 0],
  ])
  expect(
    groupTickets(tickets, "assignee").map((group) => [group.label, group.tickets.length]),
  ).toEqual([
    ["Ada", 2],
    ["Unassigned", 1],
  ])
  expect(groupTickets(tickets, "none")).toEqual([{ label: "Tickets", tickets }])
})

function ticket(patch: Partial<Ticket>): Ticket {
  return {
    id: "fer-example",
    revision: "revision",
    path: "/tmp/ticket.md",
    status: "open",
    deps: [],
    links: [],
    created: "2026-07-01",
    updated: "2026-07-01",
    type: "task",
    priority: 2,
    tags: [],
    title: "Example",
    description: "",
    raw: "",
    blocked: false,
    notes: [],
    ...patch,
  }
}
