import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import type { Editor } from "@tiptap/core"
import { fetchGitHubReferences, type GitHubReference } from "../../api/tickets"
import type { Ticket } from "../../types/ticket"
import type { EditorSuggestionMenuState } from "./editor-suggestion-menu"
import {
  editorSuggestionKey,
  mergeReferenceSuggestions,
  personSuggestions,
  ticketSuggestions,
  triggerQuery,
  type EditorSuggestion,
} from "./editor-suggestions"

export function useEditorSuggestions({
  tickets,
  currentUser,
}: {
  tickets: Ticket[]
  currentUser?: string
}) {
  const [state, setState] = useState<EditorSuggestionMenuState | null>(null)
  const [index, setIndex] = useState(0)
  const [githubItems, setGitHubItems] = useState<GitHubReference[]>([])
  const [githubAvailable, setGitHubAvailable] = useState(true)
  const [loading, setLoading] = useState(false)
  const stateRef = useRef(state)
  const indexRef = useRef(index)
  stateRef.current = state
  indexRef.current = index

  const suggestions = useMemo(() => {
    if (!state) return []
    if (state.trigger === "@") return personSuggestions(tickets, currentUser, state.query)
    return mergeReferenceSuggestions(ticketSuggestions(tickets, state.query), githubItems)
  }, [currentUser, githubItems, state, tickets])
  const suggestionsRef = useRef(suggestions)
  suggestionsRef.current = suggestions

  useEffect(() => {
    if (state?.trigger !== "#") {
      setGitHubItems([])
      setLoading(false)
      return
    }
    const controller = new AbortController()
    setGitHubItems([])
    setLoading(true)
    const timeout = window.setTimeout(() => {
      void fetchGitHubReferences(state.query, controller.signal)
        .then((response) => {
          setGitHubAvailable(response.available)
          setGitHubItems(response.items)
        })
        .catch((error) => {
          if (error instanceof DOMException && error.name === "AbortError") return
          setGitHubAvailable(false)
          setGitHubItems([])
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoading(false)
        })
    }, 240)
    return () => {
      window.clearTimeout(timeout)
      controller.abort()
    }
  }, [state?.query, state?.trigger])

  useEffect(() => setIndex(0), [state?.query, state?.trigger])

  const close = useCallback(() => {
    stateRef.current = null
    setState(null)
  }, [])

  const update = useCallback(
    (editor: Editor) => {
      const { selection } = editor.state
      if (!editor.isFocused || !selection.empty || !selection.$from.parent.isTextblock) {
        close()
        return false
      }
      const textBefore = selection.$from.parent.textBetween(
        0,
        selection.$from.parentOffset,
        undefined,
        "\ufffc",
      )
      const match = triggerQuery(textBefore)
      if (!match) {
        close()
        return false
      }
      const coords = editor.view.coordsAtPos(selection.from)
      const next: EditorSuggestionMenuState = {
        trigger: match.trigger,
        query: match.query,
        from: selection.from - match.query.length - 1,
        to: selection.from,
        left: Math.max(8, Math.min(coords.left, window.innerWidth - 360)),
        top:
          coords.bottom + 300 < window.innerHeight
            ? coords.bottom + 6
            : Math.max(8, coords.top - 294),
      }
      stateRef.current = next
      setState(next)
      return true
    },
    [close],
  )

  const select = useCallback(
    (editor: Editor, suggestion: EditorSuggestion) => {
      const current = stateRef.current
      if (!current) return
      const content = {
        type: "text",
        text: suggestion.insertText,
        ...(suggestion.href
          ? {
              marks: [
                {
                  type: "link",
                  attrs: {
                    href: suggestion.href,
                    target: suggestion.href.startsWith("http") ? "_blank" : null,
                    rel: suggestion.href.startsWith("http") ? "noopener noreferrer" : null,
                  },
                },
              ],
            }
          : {}),
      }
      editor
        .chain()
        .focus()
        .insertContentAt({ from: current.from, to: current.to }, content)
        .insertContent(" ")
        .run()
      close()
    },
    [close],
  )

  const handleKey = useCallback(
    (editor: Editor, event: KeyboardEvent) => {
      const current = stateRef.current
      if (!current) return false
      const action = editorSuggestionKey(event.key)
      if (!action) return false
      if (action === "close") {
        event.preventDefault()
        close()
        return true
      }
      const options = suggestionsRef.current
      if (action === "next" || action === "previous") {
        if (!options.length) return false
        event.preventDefault()
        setIndex((value) => (value + (action === "next" ? 1 : options.length - 1)) % options.length)
        return true
      }
      if (action === "accept") {
        const suggestion = options[indexRef.current] ?? options[0]
        if (!suggestion) {
          close()
          return false
        }
        event.preventDefault()
        select(editor, suggestion)
        return true
      }
      return false
    },
    [close, select],
  )

  return {
    state,
    suggestions,
    index,
    loading,
    githubAvailable,
    setIndex,
    select,
    update,
    close,
    handleKey,
  }
}
