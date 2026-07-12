import { useEffect, useState } from "react"
import { fetchPreferences, savePreferences } from "../api/preferences"
import type { View } from "../types/ticket"
import {
  emptyFilters,
  type FilterCondition,
  type FilterField,
  type FilterGroup,
  type FilterOperator,
  type FilterState,
} from "../features/filters/filter-types"
import type { DisplayPreferences } from "./use-display-preferences"

const views: View[] = ["active", "all", "blocked", "closed"]
const fields: FilterField[] = [
  "status",
  "priority",
  "type",
  "assignee",
  "labels",
  "blocked",
  "relation",
  "created",
]
const operators: FilterOperator[] = ["is", "is_not", "includes", "excludes", "before", "after"]
export type SavedView = {
  id: string
  name: string
  view: View
  filters: FilterState
  display: DisplayPreferences
}

function validCondition(value: unknown): value is FilterCondition {
  if (!value || typeof value !== "object") return false
  const item = value as Record<string, unknown>
  return (
    typeof item.id === "string" &&
    fields.includes(item.field as FilterField) &&
    operators.includes(item.operator as FilterOperator) &&
    typeof item.value === "string"
  )
}
function validGroup(value: unknown, depth = 0): FilterGroup | null {
  if (!value || typeof value !== "object" || depth > 4) return null
  const group = value as Record<string, unknown>
  const children = Array.isArray(group.groups)
    ? (group.groups.map((item) => validGroup(item, depth + 1)).filter(Boolean) as FilterGroup[])
    : []
  return {
    id: typeof group.id === "string" ? group.id : undefined,
    match: group.match === "any" ? "any" : "all",
    conditions: Array.isArray(group.conditions) ? group.conditions.filter(validCondition) : [],
    groups: children,
    query: typeof group.query === "string" ? group.query : "",
  }
}

function initial() {
  return { view: "active" as View, filters: emptyFilters, savedViews: [] as SavedView[] }
}

export function useWorkspacePreferences() {
  const [preferences, setPreferences] = useState(initial)
  const [hydrated, setHydrated] = useState(false)
  useEffect(() => {
    let active = true
    void fetchPreferences("workspace")
      .then((value) => {
        if (!active) return
        if (!value || typeof value !== "object") {
          setHydrated(true)
          return
        }
        try {
          const persisted = value as {
            view?: View
            filters?: FilterState
            savedViews?: SavedView[]
          }
          const parsed = validGroup(persisted.filters)
          setPreferences((current) => ({
            view: persisted.view && views.includes(persisted.view) ? persisted.view : current.view,
            filters: parsed ?? current.filters,
            savedViews: Array.isArray(persisted.savedViews)
              ? persisted.savedViews
                  .filter(
                    (item) =>
                      item &&
                      typeof item.id === "string" &&
                      typeof item.name === "string" &&
                      views.includes(item.view) &&
                      validGroup(item.filters) &&
                      item.display,
                  )
                  .map((item) => ({ ...item, filters: validGroup(item.filters)! }))
              : current.savedViews,
          }))
        } finally {
          setHydrated(true)
        }
      })
      .catch(() => {
        if (active) setHydrated(true)
      })
    return () => {
      active = false
    }
  }, [])
  useEffect(() => {
    if (!hydrated) return
    const timeout = window.setTimeout(() => {
      void savePreferences("workspace", preferences).catch(() => {
        /* Session state remains usable. */
      })
    }, 200)
    return () => window.clearTimeout(timeout)
  }, [hydrated, preferences])
  return {
    hydrated,
    view: preferences.view,
    filters: preferences.filters,
    savedViews: preferences.savedViews,
    setView: (view: View) => setPreferences((current) => ({ ...current, view })),
    setFilters: (filters: FilterState) => setPreferences((current) => ({ ...current, filters })),
    saveView: (name: string, display: DisplayPreferences) =>
      setPreferences((current) => ({
        ...current,
        savedViews: [
          ...current.savedViews,
          { id: crypto.randomUUID(), name, view: current.view, filters: current.filters, display },
        ],
      })),
    removeView: (id: string) =>
      setPreferences((current) => ({
        ...current,
        savedViews: current.savedViews.filter((view) => view.id !== id),
      })),
    applyView: (saved: SavedView) =>
      setPreferences((current) => ({ ...current, view: saved.view, filters: saved.filters })),
  }
}
