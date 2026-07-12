import { expect, test } from "bun:test"
import { filterCommandPaletteEntries, type CommandPaletteSearchEntry } from "./command-palette"

const entries: Array<CommandPaletteSearchEntry & { id: string }> = [
  {
    id: "create",
    label: "Create ticket",
    keywords: "new add create",
    detail: "C",
    group: "Actions",
  },
  {
    id: "m1",
    label: "M1 — Core platform",
    keywords: "fer-m1 milestone platform",
    detail: "fer-m1",
    group: "Tickets",
  },
  {
    id: "m2",
    label: "M2 — Web experience",
    keywords: "fer-m2 milestone web",
    detail: "fer-m2",
    group: "Tickets",
  },
]

test("command palette searches tickets from the first character", () => {
  expect(filterCommandPaletteEntries(entries, "M").map((entry) => entry.id)).toEqual(["m1", "m2"])
  expect(filterCommandPaletteEntries(entries, "m1").map((entry) => entry.id)).toEqual(["m1"])
  expect(filterCommandPaletteEntries(entries, "M2").map((entry) => entry.id)).toEqual(["m2"])
})

test("ticket matches rank before action matches while searching", () => {
  expect(filterCommandPaletteEntries(entries, "c").map((entry) => entry.id)).toEqual([
    "m1",
    "m2",
    "create",
  ])
})

test("empty search preserves launcher order", () => {
  expect(filterCommandPaletteEntries(entries, " ").map((entry) => entry.id)).toEqual([
    "create",
    "m1",
    "m2",
  ])
})
