import { useEffect, useState } from "react"
import { fetchPreferences, savePreferences } from "../api/preferences"

export type LayoutMode = "grouped" | "hierarchy" | "board" | "roadmap" | "dag"
export type Density = "comfortable" | "compact"
export type GroupBy = "status" | "priority" | "type" | "assignee" | "none"
export type OrderBy = "priority" | "created" | "title" | "id"
export type DagDirection = "LR" | "TB"
export type DisplayPreferences = {
  layout: LayoutMode
  density: Density
  groupBy: GroupBy
  orderBy: OrderBy
  descending: boolean
  showClosed: boolean
  sidebarCollapsed: boolean
  dagDirection: DagDirection
}

const defaults: DisplayPreferences = {
  layout: "grouped",
  density: "comfortable",
  groupBy: "status",
  orderBy: "priority",
  descending: false,
  showClosed: false,
  sidebarCollapsed: false,
  dagDirection: "LR",
}
function initialPreferences(): DisplayPreferences {
  return defaults
}

function sanitize(saved: Partial<DisplayPreferences>): DisplayPreferences {
  return {
    layout:
      saved.layout === "hierarchy" ||
      saved.layout === "board" ||
      saved.layout === "roadmap" ||
      saved.layout === "dag"
        ? saved.layout
        : "grouped",
    density: saved.density === "compact" ? "compact" : "comfortable",
    groupBy: ["status", "priority", "type", "assignee", "none"].includes(saved.groupBy ?? "")
      ? saved.groupBy!
      : "status",
    orderBy: ["priority", "created", "title", "id"].includes(saved.orderBy ?? "")
      ? saved.orderBy!
      : "priority",
    descending: saved.descending === true,
    showClosed: saved.showClosed === true,
    sidebarCollapsed: saved.sidebarCollapsed === true,
    dagDirection: saved.dagDirection === "TB" ? "TB" : "LR",
  }
}

export function useDisplayPreferences() {
  const [display, setDisplay] = useState<DisplayPreferences>(initialPreferences)
  const [hydrated, setHydrated] = useState(false)
  useEffect(() => {
    let active = true
    void fetchPreferences("display")
      .then((value) => {
        if (active && value && typeof value === "object")
          setDisplay(sanitize(value as Partial<DisplayPreferences>))
      })
      .catch(() => {
        /* Keep the browser fallback. */
      })
      .finally(() => {
        if (active) setHydrated(true)
      })
    return () => {
      active = false
    }
  }, [])
  useEffect(() => {
    if (!hydrated) return
    const timeout = window.setTimeout(() => {
      void savePreferences("display", display).catch(() => {
        /* Session state remains usable. */
      })
    }, 200)
    return () => window.clearTimeout(timeout)
  }, [display, hydrated])
  const updateDisplay = (patch: Partial<DisplayPreferences>) =>
    setDisplay((value) => ({ ...value, ...patch }))
  return { hydrated, display, updateDisplay }
}
