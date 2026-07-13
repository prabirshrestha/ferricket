import { expect, test } from "bun:test"
import type { Ticket } from "../../types/ticket"
import { aggregateInsights, aggregateTrend } from "./insights-view"
import { nearestTrendPointIndex, ticketsTouchedInWindow } from "./insights-data"

const ticket = (patch: Partial<Ticket>): Ticket => ({
  id: "fer-1",
  revision: "r",
  path: "/tmp/fer-1.md",
  status: "open",
  deps: [],
  links: [],
  created: "2026-01-01T00:00:00Z",
  updated: "2026-01-01T00:00:00Z",
  type: "task",
  priority: 2,
  tags: [],
  title: "T",
  description: "",
  raw: "",
  blocked: false,
  notes: [],
  ...patch,
})

test("aggregates categorical ticket insights and multi-valued labels", () => {
  const tickets = [
    ticket({ id: "a", status: "open", tags: ["web", "ux"] }),
    ticket({ id: "b", status: "closed", tags: ["web"] }),
    ticket({ id: "c", status: "open" }),
  ]
  expect(aggregateInsights(tickets, "status").map((row) => [row.label, row.value])).toEqual([
    ["Todo", 2],
    ["Done", 1],
  ])
  expect(aggregateInsights(tickets, "labels").map((row) => [row.label, row.value])).toEqual([
    ["web", 2],
    ["No label", 1],
    ["ux", 1],
  ])
})

test("trend aggregation tolerates missing dates and tracks daily state", () => {
  const result = aggregateTrend(
    [
      ticket({ id: "created", created: "2026-01-02T00:00:00Z", updated: undefined }),
      ticket({
        id: "done",
        status: "closed",
        created: "2026-01-01T00:00:00Z",
        updated: "2026-01-03T00:00:00Z",
      }),
      ticket({ id: "legacy", created: "", updated: undefined }),
    ],
    {
      range: "custom",
      bin: "1d",
      customStart: "2026-01-01T00:00:00Z",
      customEnd: "2026-01-05T00:00:00Z",
      now: new Date("2026-01-05T00:00:00Z"),
    },
  )

  expect(
    result.points.map((point) => [
      point.timestamp.slice(0, 10),
      point.created,
      point.completed,
      point.inProgress,
      point.open,
    ]),
  ).toEqual([
    ["2026-01-01", 1, 0, 0, 1],
    ["2026-01-02", 1, 0, 0, 2],
    ["2026-01-03", 0, 1, 0, 1],
    ["2026-01-04", 0, 0, 0, 1],
  ])
})

test("trend approximates in-progress activity from the update timestamp", () => {
  const result = aggregateTrend(
    [
      ticket({
        id: "active",
        status: "in_progress",
        created: "2026-01-01T00:00:00Z",
        updated: "2026-01-03T00:00:00Z",
      }),
    ],
    {
      range: "custom",
      bin: "1d",
      customStart: "2026-01-01T00:00:00Z",
      customEnd: "2026-01-05T00:00:00Z",
    },
  )
  expect(result.points.map((point) => point.inProgress)).toEqual([0, 0, 1, 0])
})

test("smart range begins near the first relevant event", () => {
  const now = new Date("2026-06-10T12:00:00Z")
  const result = aggregateTrend(
    [
      ticket({
        created: "2026-06-09T12:00:00Z",
        updated: "2026-06-10T09:00:00Z",
      }),
    ],
    { range: "smart", bin: "auto", now },
  )
  expect(result.start.getTime()).toBeGreaterThan(new Date("2026-06-09T10:00:00Z").getTime())
  expect(result.start.getTime()).toBeLessThan(new Date("2026-06-09T12:00:00Z").getTime())
  expect(result.end.toISOString()).toBe(now.toISOString())
})

test("auto bins adapt across short and long windows", () => {
  const now = new Date("2026-06-10T12:00:00Z")
  expect(aggregateTrend([], { range: "1h", bin: "auto", now }).binMinutes).toBe(1)
  expect(aggregateTrend([], { range: "24h", bin: "auto", now }).binMinutes).toBe(30)
  expect(aggregateTrend([], { range: "7d", bin: "auto", now }).binMinutes).toBe(1440)
})

test("small requested bins are capped to a bounded chart", () => {
  const result = aggregateTrend([], {
    range: "90d",
    bin: "1m",
    now: new Date("2026-06-10T12:00:00Z"),
  })
  expect(result.requestedBinMinutes).toBe(1)
  expect(result.binMinutes).toBeGreaterThan(1)
  expect(result.points.length).toBeLessThanOrEqual(600)
  expect(result.limited).toBe(true)
})

test("custom bins are rounded and clamped to valid minutes", () => {
  const now = new Date("2026-06-10T12:00:00Z")
  expect(
    aggregateTrend([], { range: "1h", bin: "custom", customMinutes: 12.6, now })
      .requestedBinMinutes,
  ).toBe(13)
  expect(
    aggregateTrend([], { range: "1h", bin: "custom", customMinutes: 0, now }).requestedBinMinutes,
  ).toBe(1)
})

test("the shared insights window includes tickets created or updated inside it", () => {
  const tickets = [
    ticket({ id: "created", created: "2026-06-10T10:00:00Z" }),
    ticket({
      id: "updated",
      created: "2026-05-01T10:00:00Z",
      updated: "2026-06-10T11:00:00Z",
    }),
    ticket({ id: "outside", created: "2026-05-01T10:00:00Z" }),
  ]
  expect(
    ticketsTouchedInWindow(
      tickets,
      new Date("2026-06-10T09:00:00Z"),
      new Date("2026-06-10T12:00:00Z"),
    ).map((item) => item.id),
  ).toEqual(["created", "updated"])
})

test("chart inspection snaps to and clamps at the nearest trend point", () => {
  expect(nearestTrendPointIndex(48, 5, 48, 884)).toBe(0)
  expect(nearestTrendPointIndex(466, 5, 48, 884)).toBe(2)
  expect(nearestTrendPointIndex(884, 5, 48, 884)).toBe(4)
  expect(nearestTrendPointIndex(-100, 5, 48, 884)).toBe(0)
  expect(nearestTrendPointIndex(1_000, 5, 48, 884)).toBe(4)
})
