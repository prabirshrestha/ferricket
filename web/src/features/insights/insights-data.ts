import { statusMeta, type Status, type Ticket } from "../../types/ticket"

export type InsightSlice = "status" | "priority" | "type" | "assignee" | "labels"
export type InsightRow = { key: string; label: string; value: number }
export type TrendSeries = "created" | "completed" | "inProgress" | "open"
export type TrendRange = "smart" | "1h" | "24h" | "7d" | "30d" | "90d" | "custom"
export type TrendBin = "auto" | "1m" | "30m" | "1d" | "custom"
export type TrendPoint = {
  timestamp: string
  created: number
  completed: number
  inProgress: number
  open: number
}
export type TrendOptions = {
  range: TrendRange
  bin: TrendBin
  customStart?: string
  customEnd?: string
  customMinutes?: number
  now?: Date
}
export type TrendResult = {
  points: TrendPoint[]
  start: Date
  end: Date
  binMinutes: number
  requestedBinMinutes: number
  limited: boolean
}

const MAX_BINS = 600

export function nearestTrendPointIndex(
  position: number,
  pointCount: number,
  left: number,
  right: number,
) {
  if (pointCount <= 1 || right <= left) return 0
  const ratio = Math.max(0, Math.min(1, (position - left) / (right - left)))
  return Math.round(ratio * (pointCount - 1))
}

export function aggregateInsights(tickets: Ticket[], slice: InsightSlice): InsightRow[] {
  const values = new Map<string, { label: string; value: number }>()
  const add = (key: string, label = key) => {
    const current = values.get(key)
    values.set(key, { label, value: (current?.value ?? 0) + 1 })
  }
  for (const ticket of tickets) {
    if (slice === "labels") {
      if (ticket.tags.length === 0) add("__none__", "No label")
      else ticket.tags.forEach((tag) => add(tag))
    } else if (slice === "status") add(ticket.status, statusMeta[ticket.status as Status].label)
    else if (slice === "priority") add(String(ticket.priority), `P${ticket.priority}`)
    else if (slice === "assignee")
      add(ticket.assignee || "__none__", ticket.assignee || "Unassigned")
    else add(ticket.type || "task", capitalize(ticket.type || "task"))
  }
  return [...values]
    .map(([key, row]) => ({ key, ...row }))
    .sort((a, b) => b.value - a.value || a.label.localeCompare(b.label))
}

export function aggregateTrend(tickets: Ticket[], options: TrendOptions): TrendResult {
  const { start, end } = resolveTrendWindow(tickets, options)
  const requestedBinMinutes = resolveRequestedBin(options, start, end)
  const spanMinutes = Math.max(1, (end.getTime() - start.getTime()) / 60_000)
  const binMinutes = Math.max(requestedBinMinutes, Math.ceil(spanMinutes / MAX_BINS))
  const binMs = binMinutes * 60_000
  const count = Math.max(1, Math.ceil((end.getTime() - start.getTime()) / binMs))
  const buckets = Array.from({ length: count }, (_, index) => ({
    timestamp: new Date(start.getTime() + index * binMs).toISOString(),
    created: 0,
    completed: 0,
    inProgress: 0,
    open: 0,
  }))

  let open = 0
  for (const ticket of tickets) {
    const created = parseDate(ticket.created)
    const updated = parseDate(ticket.updated || ticket.created)
    const completed = ticket.status === "closed" ? updated : null
    if (created && created < start && (!completed || completed >= start)) open += 1
    incrementEvent(buckets, start, end, binMs, created, "created")
    if (ticket.status === "closed") incrementEvent(buckets, start, end, binMs, updated, "completed")
    if (ticket.status === "in_progress")
      incrementEvent(buckets, start, end, binMs, updated, "inProgress")
  }

  for (const bucket of buckets) {
    open = Math.max(0, open + bucket.created - bucket.completed)
    bucket.open = open
  }
  return {
    points: buckets,
    start,
    end,
    binMinutes,
    requestedBinMinutes,
    limited: binMinutes !== requestedBinMinutes,
  }
}

export function resolveTrendWindow(tickets: Ticket[], options: TrendOptions) {
  const now = validDate(options.now) ?? new Date()
  const eventDates = tickets
    .flatMap((ticket) => [parseDate(ticket.created), parseDate(ticket.updated || ticket.created)])
    .filter((date): date is Date => Boolean(date))
  const [start, end] = resolveRange(options, eventDates, now)
  return { start, end }
}

export function ticketsTouchedInWindow(tickets: Ticket[], start: Date, end: Date) {
  return tickets.filter((ticket) => {
    const created = parseDate(ticket.created)
    const updated = parseDate(ticket.updated || ticket.created)
    return [created, updated].some((date) => date && date >= start && date <= end)
  })
}

function resolveRange(options: TrendOptions, eventDates: Date[], now: Date): [Date, Date] {
  if (options.range === "custom") {
    const start = parseDate(options.customStart || "")
    const end = parseDate(options.customEnd || "")
    if (start && end && end > start) return [start, end]
  }
  const fixedMinutes: Partial<Record<TrendRange, number>> = {
    "1h": 60,
    "24h": 24 * 60,
    "7d": 7 * 24 * 60,
    "30d": 30 * 24 * 60,
    "90d": 90 * 24 * 60,
  }
  const minutes = fixedMinutes[options.range]
  if (minutes) return [new Date(now.getTime() - minutes * 60_000), now]
  if (!eventDates.length) return [new Date(now.getTime() - 24 * 60 * 60_000), now]
  const earliest = Math.min(...eventDates.map((date) => date.getTime()))
  const latest = Math.max(now.getTime(), ...eventDates.map((date) => date.getTime()))
  const padding = Math.max(60_000, (latest - earliest) * 0.03)
  return [new Date(earliest - padding), new Date(latest)]
}

function resolveRequestedBin(options: TrendOptions, start: Date, end: Date) {
  if (options.bin === "1m") return 1
  if (options.bin === "30m") return 30
  if (options.bin === "1d") return 24 * 60
  if (options.bin === "custom")
    return Math.max(1, Math.min(43_200, Math.round(options.customMinutes || 1)))
  const spanMinutes = (end.getTime() - start.getTime()) / 60_000
  if (spanMinutes <= 3 * 60) return 1
  if (spanMinutes <= 3 * 24 * 60) return 30
  return 24 * 60
}

function incrementEvent(
  buckets: TrendPoint[],
  start: Date,
  end: Date,
  binMs: number,
  date: Date | null,
  key: "created" | "completed" | "inProgress",
) {
  if (!date || date < start || date > end) return
  const index = Math.min(buckets.length - 1, Math.floor((date.getTime() - start.getTime()) / binMs))
  buckets[index][key] += 1
}

function parseDate(value: string) {
  const date = new Date(value)
  return validDate(date)
}
function validDate(value?: Date | null) {
  return value && !Number.isNaN(value.getTime()) ? value : null
}
function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1)
}
