import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import {
  dependencyConnections,
  dependencyTopologyKey,
  layoutDependencyDag,
  prerequisitePath,
} from "./dependency-dag"

const ticket = (id: string, deps: string[] = []): Ticket => ({
  id,
  deps,
  revision: id,
  path: id + ".md",
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

test("DAG connections point from prerequisites to dependents", () => {
  const connections = dependencyConnections([
    ticket("foundation"),
    ticket("feature", ["foundation", "outside", "foundation"]),
  ])
  expect(connections).toEqual([
    { id: "foundation->feature", source: "foundation", target: "feature" },
  ])
})

test("DAG topology ignores content-only changes but detects dependency changes", () => {
  const first = dependencyTopologyKey([
    { id: "a", deps: [] },
    { id: "b", deps: ["a"], title: "Before" },
  ])
  const contentUpdate = dependencyTopologyKey([
    { id: "b", deps: ["a"], title: "After" },
    { id: "a", deps: [] },
  ])
  const dependencyUpdate = dependencyTopologyKey([
    { id: "a", deps: [] },
    { id: "b", deps: [] },
  ])

  expect(contentUpdate).toBe(first)
  expect(dependencyUpdate).not.toBe(first)
})

test("focused path includes every recursive prerequisite once", () => {
  const tickets = [
    ticket("base"),
    ticket("left", ["base"]),
    ticket("right", ["base"]),
    ticket("final", ["left", "right"]),
    ticket("unrelated"),
  ]
  expect([...prerequisitePath("final", tickets)].sort()).toEqual(["base", "final", "left", "right"])
})

test("automatic DAG layout places prerequisites to the left", () => {
  const tickets = [ticket("base"), ticket("feature", ["base"]), ticket("final", ["feature"])]
  const positions = layoutDependencyDag(tickets)
  expect(positions.get("base")!.x).toBeLessThan(positions.get("feature")!.x)
  expect(positions.get("feature")!.x).toBeLessThan(positions.get("final")!.x)
})

test("top-to-bottom DAG layout places prerequisites above dependents", () => {
  const tickets = [ticket("base"), ticket("feature", ["base"]), ticket("final", ["feature"])]
  const positions = layoutDependencyDag(tickets, dependencyConnections(tickets), "TB")
  expect(positions.get("base")!.y).toBeLessThan(positions.get("feature")!.y)
  expect(positions.get("feature")!.y).toBeLessThan(positions.get("final")!.y)
})
