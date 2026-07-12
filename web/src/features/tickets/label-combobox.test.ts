import { describe, expect, test } from "bun:test"
import { labelItems, labelOptions } from "./label-combobox"

describe("label combobox options", () => {
  test("trims, case-insensitively deduplicates, and retains selected labels", () => {
    expect(labelOptions([" bug ", "Bug", "frontend"], ["release", "BUG"])).toEqual([
      "BUG",
      "frontend",
      "release",
    ])
  })

  test("adds the query as a creatable item only when it is new", () => {
    expect(labelItems(["bug", "frontend"], " release ")).toEqual(["bug", "frontend", "release"])
    expect(labelItems(["bug", "frontend"], "BUG")).toEqual(["bug", "frontend"])
    expect(labelItems(["bug", "frontend"], "  ")).toEqual(["bug", "frontend"])
  })
})
