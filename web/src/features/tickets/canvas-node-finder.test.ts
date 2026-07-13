import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import { canvasNodeMatches } from "./canvas-node-finder"

const ticket = {
  id: "fer-ab12",
  title: "Build dependency navigation",
  status: "in_progress",
  assignee: "Prabir",
  type: "feature",
  tags: ["web", "navigation"],
} as Ticket

test("canvas node finder matches IDs, titles, labels, assignees, and status", () => {
  expect(canvasNodeMatches(ticket, "ab12")).toBe(true)
  expect(canvasNodeMatches(ticket, "dependency nav")).toBe(true)
  expect(canvasNodeMatches(ticket, "web")).toBe(true)
  expect(canvasNodeMatches(ticket, "prabir")).toBe(true)
  expect(canvasNodeMatches(ticket, "in progress")).toBe(true)
  expect(canvasNodeMatches(ticket, "unrelated")).toBe(false)
})
