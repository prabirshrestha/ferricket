import type { Ticket } from "../../types/ticket"

export type RoadmapStage = { index: number; attention: boolean; tickets: Ticket[] }
export type RoadmapEdge = { from: string; to: string; context?: boolean }
export type DependencyRoadmap = {
  stages: RoadmapStage[]
  edges: RoadmapEdge[]
  missing: Map<string, string[]>
  unresolved: Set<string>
}

export function withDependencyContext(matched: Ticket[], all: Ticket[]) {
  const byId = new Map(all.map((ticket) => [ticket.id, ticket]))
  const included = new Map(matched.map((ticket) => [ticket.id, ticket]))
  const visited = new Set<string>()
  const pending = [...matched]
  while (pending.length) {
    const ticket = pending.pop()!
    if (visited.has(ticket.id)) continue
    visited.add(ticket.id)
    for (const dependency of ticket.deps) {
      const candidate = byId.get(dependency)
      if (!candidate) continue
      included.set(candidate.id, candidate)
      if (!visited.has(candidate.id)) pending.push(candidate)
    }
  }
  return [...included.values()]
}

export function buildDependencyRoadmap(tickets: Ticket[], contextIds = new Set<string>()) {
  const ordered = [...tickets].sort(
    (left, right) => left.priority - right.priority || left.id.localeCompare(right.id),
  )
  const byId = new Map(ordered.map((ticket) => [ticket.id, ticket]))
  const dependents = new Map<string, string[]>()
  const indegree = new Map<string, number>()
  const missing = new Map<string, string[]>()
  const edges: RoadmapEdge[] = []

  for (const ticket of ordered) {
    const dependencies = [...new Set(ticket.deps)]
    const known = dependencies.filter((id) => byId.has(id))
    const absent = dependencies.filter((id) => !byId.has(id))
    indegree.set(ticket.id, known.length)
    if (absent.length) missing.set(ticket.id, absent)
    for (const dependency of known) {
      dependents.set(dependency, [...(dependents.get(dependency) ?? []), ticket.id])
      edges.push({
        from: dependency,
        to: ticket.id,
        context: contextIds.has(dependency) || contextIds.has(ticket.id),
      })
    }
  }

  const stageById = new Map<string, number>()
  const queue = ordered.filter((ticket) => indegree.get(ticket.id) === 0).map((ticket) => ticket.id)
  queue.forEach((id) => stageById.set(id, 0))
  while (queue.length) {
    const id = queue.shift()!
    const nextStage = (stageById.get(id) ?? 0) + 1
    for (const dependent of dependents.get(id) ?? []) {
      stageById.set(dependent, Math.max(stageById.get(dependent) ?? 0, nextStage))
      const remaining = (indegree.get(dependent) ?? 0) - 1
      indegree.set(dependent, remaining)
      if (remaining === 0) queue.push(dependent)
    }
  }

  const unresolved = new Set<string>([
    ...missing.keys(),
    ...ordered.filter((ticket) => !stageById.has(ticket.id)).map((ticket) => ticket.id),
  ])
  const unresolvedQueue = [...unresolved]
  while (unresolvedQueue.length) {
    const id = unresolvedQueue.shift()!
    for (const dependent of dependents.get(id) ?? []) {
      if (unresolved.has(dependent)) continue
      unresolved.add(dependent)
      unresolvedQueue.push(dependent)
    }
  }

  const ticketsByStage = new Map<number, Ticket[]>()
  for (const ticket of ordered) {
    if (unresolved.has(ticket.id)) continue
    const stage = stageById.get(ticket.id) ?? 0
    ticketsByStage.set(stage, [...(ticketsByStage.get(stage) ?? []), ticket])
  }
  const stages: RoadmapStage[] = [...ticketsByStage.entries()]
    .sort(([left], [right]) => left - right)
    .map(([, stageTickets], index) => ({
      index,
      attention: false,
      tickets: stageTickets,
    }))
  if (unresolved.size)
    stages.push({
      index: stages.length,
      attention: true,
      tickets: ordered.filter((ticket) => unresolved.has(ticket.id)),
    })

  return { stages, edges, missing, unresolved }
}
