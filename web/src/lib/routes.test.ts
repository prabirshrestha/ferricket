import { expect, test } from "bun:test"
import { insightsRoute, parseAppRoute, savedViewRoute, ticketRoute, viewRoute } from "./routes"

test("parses bookmarkable built-in, saved-view, and ticket routes", () => {
  expect(parseAppRoute("/blocked")).toEqual({ view: "blocked", ticketId: undefined })
  expect(parseAppRoute("/all/ticket/fer-123")).toEqual({ view: "all", ticketId: "fer-123" })
  expect(parseAppRoute("/views/my%20view/ticket/fer-456")).toEqual({
    savedViewId: "my view",
    ticketId: "fer-456",
  })
  expect(parseAppRoute("/ticket/fer-789")).toEqual({ ticketId: "fer-789" })
  expect(parseAppRoute("/insights")).toEqual({ insights: true })
  expect(parseAppRoute("/all/ticket/%E0%A4%A")).toEqual({ view: "all", ticketId: "%E0%A4%A" })
  expect(viewRoute("active")).toBe("/active")
  expect(insightsRoute()).toBe("/insights")
  expect(savedViewRoute("a b")).toBe("/views/a%20b")
  expect(ticketRoute("/active", "a b")).toBe("/active/ticket/a%20b")
})
