import { AtSign, CircleDot, GitPullRequest, TicketIcon } from "lucide-react"
import { createPortal } from "react-dom"
import { cn } from "../../lib/utils"
import type { EditorSuggestion, EditorTrigger } from "./editor-suggestions"

export type EditorSuggestionMenuState = {
  trigger: EditorTrigger
  query: string
  from: number
  to: number
  left: number
  top: number
}

export function EditorSuggestionMenu({
  state,
  suggestions,
  index,
  loading,
  githubAvailable,
  menuId,
  onIndex,
  onSelect,
}: {
  state: EditorSuggestionMenuState | null
  suggestions: EditorSuggestion[]
  index: number
  loading: boolean
  githubAvailable: boolean
  menuId: string
  onIndex: (index: number) => void
  onSelect: (suggestion: EditorSuggestion) => void
}) {
  if (!state) return null
  const empty = !loading && suggestions.length === 0
  return createPortal(
    <div
      data-slot="editor-suggestion-menu"
      id={menuId}
      className="fixed z-[80] w-[min(22rem,calc(100vw-1rem))] overflow-hidden rounded-lg bg-popover text-popover-foreground shadow-xl ring-1 ring-foreground/10"
      style={{ left: state.left, top: state.top }}
      role="listbox"
      aria-label={state.trigger === "@" ? "Mention a person" : "Reference a ticket or GitHub item"}
    >
      <div className="flex items-center border-b px-3 py-2 text-[11px] text-muted-foreground">
        <span>{state.trigger === "@" ? "People" : "Tickets, issues, and pull requests"}</span>
        <span className="ml-auto">↑↓ · Tab to insert</span>
      </div>
      <div className="max-h-72 overflow-y-auto p-1.5">
        {suggestions.map((suggestion, itemIndex) => {
          const Icon = suggestionIcon(suggestion.kind)
          return (
            <button
              id={`${menuId}-option-${itemIndex}`}
              type="button"
              role="option"
              aria-selected={itemIndex === index}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left outline-none",
                itemIndex === index && "bg-accent text-accent-foreground",
              )}
              key={suggestion.id}
              onMouseEnter={() => onIndex(itemIndex)}
              onMouseDown={(event) => {
                event.preventDefault()
                onSelect(suggestion)
              }}
            >
              <span className="grid size-7 shrink-0 place-items-center rounded-md border bg-background">
                <Icon className="size-3.5 text-muted-foreground" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-xs font-medium">{suggestion.label}</span>
                <span className="block truncate text-[11px] text-muted-foreground">
                  {suggestion.detail}
                </span>
              </span>
              {state.trigger === "#" && (
                <span className="shrink-0 text-[10px] capitalize text-muted-foreground">
                  {suggestion.kind.replace("_", " ")}
                </span>
              )}
            </button>
          )
        })}
        {(loading || empty) && (
          <div className="px-3 py-6 text-center text-xs text-muted-foreground">
            {loading
              ? "Searching GitHub…"
              : state.trigger === "#" && !githubAvailable
                ? "No local matches. Connect gh to this repository for GitHub results."
                : "No matches."}
          </div>
        )}
      </div>
    </div>,
    document.body,
  )
}

function suggestionIcon(kind: EditorSuggestion["kind"]) {
  if (kind === "person") return AtSign
  if (kind === "issue") return CircleDot
  if (kind === "pull_request") return GitPullRequest
  return TicketIcon
}
