import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import { buildDependencyRoadmap, withDependencyContext } from "./dependency-roadmap"

const ticket = (id: string, deps: string[] = []): Ticket => ({
  id,
  deps,
  revision: id,
  path: `${id}.md`,
  status: "open",
  links: [],
  created: "2026-07-01",
  type: "task",
  priority: 2,
  tags: [],
  title: id,
  description: "",
  raw: "",
  blocked: false,
  notes: [],
})

test("dependency roadmap stages parallel work before its dependents", () => {
  const roadmap = buildDependencyRoadmap([
    ticket("a"),
    ticket("b"),
    ticket("c", ["a"]),
    ticket("d", ["a", "b"]),
    ticket("e", ["c", "d"]),
  ])
  expect(roadmap.stages.map((stage) => stage.tickets.map((item) => item.id))).toEqual([
    ["a", "b"],
    ["c", "d"],
    ["e"],
  ])
  expect(roadmap.edges).toContainEqual({ from: "a", to: "d", context: false })
})

test("missing prerequisites and cycles are grouped for attention", () => {
  const roadmap = buildDependencyRoadmap([
    ticket("ready"),
    ticket("missing", ["outside"]),
    ticket("downstream", ["missing"]),
    ticket("cycle-a", ["cycle-b"]),
    ticket("cycle-b", ["cycle-a"]),
  ])
  expect(roadmap.stages[0].tickets.map((item) => item.id)).toEqual(["ready"])
  expect(roadmap.stages.at(-1)?.attention).toBe(true)
  expect(roadmap.unresolved).toEqual(new Set(["missing", "downstream", "cycle-a", "cycle-b"]))
  expect(roadmap.missing.get("missing")).toEqual(["outside"])
})

test("filtered roadmap retains recursive prerequisite context", () => {
  const a = ticket("a")
  const b = ticket("b", ["a"])
  const c = ticket("c", ["b"])
  expect(
    withDependencyContext([c], [a, b, c])
      .map((item) => item.id)
      .sort(),
  ).toEqual(["a", "b", "c"])
})

test("dependency context visits converging prerequisite branches once", () => {
  let dependencyReads = 0
  const shared = ticket("shared")
  Object.defineProperty(shared, "deps", {
    get() {
      dependencyReads += 1
      return []
    },
  })
  const left = ticket("left", ["shared"])
  const right = ticket("right", ["shared"])
  const target = ticket("target", ["left", "right"])

  expect(
    withDependencyContext([target], [shared, left, right, target])
      .map((item) => item.id)
      .sort(),
  ).toEqual(["left", "right", "shared", "target"])
  expect(dependencyReads).toBe(1)
})
