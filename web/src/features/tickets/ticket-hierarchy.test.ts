import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import { buildHierarchy, withAncestorContext } from "./ticket-hierarchy"

const ticket = (id: string, parent?: string): Ticket => ({
  id,
  revision: `revision-${id}`,
  path: `/tmp/.tickets/${id}.md`,
  parent,
  status: "open",
  deps: [],
  links: [],
  created: "2026-07-12T00:00:00Z",
  updated: "2026-07-12T00:00:00Z",
  type: "task",
  priority: 2,
  tags: [],
  title: id,
  description: "",
  raw: "",
  blocked: false,
  notes: [],
})

test("withAncestorContext retains filtered ticket ancestors", () => {
  const parent = ticket("parent")
  const child = { ...ticket("child"), parent: parent.id }
  const grandchild = { ...ticket("grandchild"), parent: child.id }
  const unrelated = ticket("other")
  expect(
    withAncestorContext([grandchild], [unrelated, grandchild, parent, child])
      .map((item) => item.id)
      .sort(),
  ).toEqual(["child", "grandchild", "parent"])
})

test("buildHierarchy nests children and retains orphaned or cyclic tickets once", () => {
  const tree = buildHierarchy([
    ticket("root"),
    ticket("child", "root"),
    ticket("orphan", "missing"),
    ticket("a", "b"),
    ticket("b", "a"),
  ])
  const flattened = tree.flatMap(function visit(node): string[] {
    return [node.ticket.id, ...node.children.flatMap(visit)]
  })
  expect(tree.find((node) => node.ticket.id === "root")?.children[0].ticket.id).toBe("child")
  expect(flattened.sort()).toEqual(["a", "b", "child", "orphan", "root"])
})
