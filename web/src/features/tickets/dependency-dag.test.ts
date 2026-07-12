import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import {
  dependencyBackbone,
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

test("dependency backbone removes shortcut links without changing reachability", () => {
  const tickets = [
    ticket("foundation"),
    ticket("feature", ["foundation"]),
    ticket("release", ["foundation", "feature"]),
  ]
  expect(dependencyBackbone(tickets, dependencyConnections(tickets))).toEqual([
    { id: "foundation->feature", source: "foundation", target: "feature" },
    { id: "feature->release", source: "feature", target: "release" },
  ])
})

test("dependency backbone keeps every branch that contributes to a merge", () => {
  const tickets = [
    ticket("foundation"),
    ticket("left", ["foundation"]),
    ticket("right", ["foundation"]),
    ticket("release", ["left", "right"]),
  ]
  expect(dependencyBackbone(tickets, dependencyConnections(tickets))).toHaveLength(4)
})

test("dependency backbone preserves cyclic data so the invalid links stay visible", () => {
  const tickets = [ticket("left", ["right"]), ticket("right", ["left"])]
  const connections = dependencyConnections(tickets)
  expect(dependencyBackbone(tickets, connections)).toEqual(connections)
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

test("automatic DAG layout is stable when ticket input order changes", () => {
  const first = [ticket("base"), ticket("left", ["base"]), ticket("right", ["base"])]
  const second = [first[2], first[0], first[1]]
  expect(Object.fromEntries(layoutDependencyDag(second))).toEqual(
    Object.fromEntries(layoutDependencyDag(first)),
  )
})
