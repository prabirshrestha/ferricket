import { describe, expect, test } from "bun:test"
import { slashQuery } from "./markdown-editor"

describe("slash command query", () => {
  test("finds a command token at the caret", () => {
    expect(slashQuery("Start a /hea")).toEqual({ query: "hea", startOffset: 8 })
    expect(slashQuery("/")).toEqual({ query: "", startOffset: 0 })
  })

  test("does not open inside words or after completed text", () => {
    expect(slashQuery("https://example.test")).toBeNull()
    expect(slashQuery("/heading done")).toBeNull()
  })
})
