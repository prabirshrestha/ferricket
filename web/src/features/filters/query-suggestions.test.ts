import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import { applySuggestion, querySuggestions, tokenAt } from "./query-suggestions"

const ticket = {
  id: "fer-1",
  title: "Build filters",
  assignee: "Ada Lovelace",
  tags: ["front end"],
  type: "feature",
} as Ticket

test("finds and replaces the token at the caret", () => {
  expect(tokenAt("status:todo label:fro", 21)).toEqual({ start: 12, end: 21, token: "label:fro" })
  const label = querySuggestions("status:todo label:fro", [ticket]).find((item) =>
    item.id.includes("front end"),
  )!
  expect(applySuggestion("status:todo label:fro", label)).toEqual({
    value: 'status:todo label:"front end" ',
    cursor: 30,
  })
  expect(tokenAt('status:todo title:"Build fil" label:web', 27)).toEqual({
    start: 12,
    end: 29,
    token: 'title:"Build fil"',
  })
  expect(tokenAt('status:todo title:"Build fil', 28)).toEqual({
    start: 12,
    end: 28,
    token: 'title:"Build fil',
  })
  expect(querySuggestions("-sta", [ticket])[0].replacement).toBe("-status:")
})

test("suggests fields, predicates, and live workspace values", () => {
  expect(querySuggestions("ass", [ticket])[0].replacement).toBe("assignee:")
  expect(
    querySuggestions("assignee:a", [ticket]).some((item) =>
      item.replacement.includes("Ada Lovelace"),
    ),
  ).toBe(true)
  expect(querySuggestions("is:b", [ticket]).some((item) => item.replacement === "is:blocked")).toBe(
    true,
  )
  expect(querySuggestions("status:", [ticket]).map((item) => item.replacement)).toEqual([
    "status:todo",
    "status:active",
    "status:done",
  ])
  expect(querySuggestions("tag", [ticket]).some((item) => item.replacement === "tag:")).toBe(true)
  expect(
    querySuggestions("tag:f", [ticket]).some((item) => item.replacement === 'tag:"front end"'),
  ).toBe(true)
})

test("completion consumes existing separator whitespace", () => {
  const suggestion = querySuggestions("status:todo", [ticket])[0]
  expect(applySuggestion("status:todo  label:web", suggestion, 11)).toEqual({
    value: "status:todo label:web",
    cursor: 12,
  })
})

test("field completion chains into value completion", () => {
  const field = querySuggestions("sta", [ticket])[0]
  expect(field.replacement).toBe("status:")
  expect(field.complete).toBeFalsy()
  const completedField = applySuggestion("sta", field)
  expect(completedField.value).toBe("status:")
  expect(querySuggestions(completedField.value, [ticket], completedField.cursor)[0]).toMatchObject({
    label: "todo",
    complete: true,
  })
})
