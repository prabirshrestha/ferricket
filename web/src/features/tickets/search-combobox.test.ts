import { describe, expect, test } from "bun:test"
import { searchOptionMatches, type SearchOption } from "./search-combobox"

const option: SearchOption = {
  value: "tr-w2mh",
  label: "M1.5 — Binary protocol completion",
  detail: "tr-w2mh",
}

describe("search combobox matching", () => {
  test("matches labels and ticket IDs without regard to case", () => {
    expect(searchOptionMatches(option, "binary protocol")).toBe(true)
    expect(searchOptionMatches(option, "TR-W2")).toBe(true)
  })

  test("ignores surrounding whitespace and rejects unrelated text", () => {
    expect(searchOptionMatches(option, "  m1.5  ")).toBe(true)
    expect(searchOptionMatches(option, "browser security")).toBe(false)
  })
})
