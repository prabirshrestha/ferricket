import { expect, test } from "bun:test"
import {
  insightsRoute,
  parseAppRoute,
  rawTicketRoute,
  savedViewRoute,
  ticketRoute,
  viewRoute,
} from "./routes"

test("parses bookmarkable built-in, saved-view, and ticket routes", () => {
  expect(parseAppRoute("/blocked")).toEqual({
    view: "blocked",
    ticketId: undefined,
    rawTicket: false,
  })
  expect(parseAppRoute("/all/ticket/fer-123")).toEqual({
    view: "all",
    ticketId: "fer-123",
    rawTicket: false,
  })
  expect(parseAppRoute("/views/my%20view/ticket/fer-456")).toEqual({
    savedViewId: "my view",
    ticketId: "fer-456",
    rawTicket: false,
  })
  expect(parseAppRoute("/ticket/fer-789")).toEqual({ ticketId: "fer-789", rawTicket: false })
  expect(parseAppRoute("/active/ticket/fer-789/raw")).toEqual({
    view: "active",
    ticketId: "fer-789",
    rawTicket: true,
  })
  expect(parseAppRoute("/insights")).toEqual({ insights: true })
  expect(parseAppRoute("/all/ticket/%E0%A4%A")).toEqual({
    view: "all",
    ticketId: "%E0%A4%A",
    rawTicket: false,
  })
  expect(viewRoute("active")).toBe("/active")
  expect(insightsRoute()).toBe("/insights")
  expect(savedViewRoute("a b")).toBe("/views/a%20b")
  expect(ticketRoute("/active", "a b")).toBe("/active/ticket/a%20b")
  expect(rawTicketRoute("/active", "a b")).toBe("/active/ticket/a%20b/raw")
})
