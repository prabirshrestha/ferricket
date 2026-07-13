import { createPortal } from "react-dom"
import { cn } from "../../lib/utils"

export type SlashMenuState = { query: string; from: number; to: number; left: number; top: number }

export type EditorSlashCommand = {
  label: string
  description: string
  keywords: string
  icon: React.ComponentType<{ className?: string }>
}

export function EditorSlashMenu<T extends EditorSlashCommand>({
  state,
  commands,
  index,
  menuId,
  onIndex,
  onSelect,
}: {
  state: SlashMenuState | null
  commands: T[]
  index: number
  menuId: string
  onIndex: (index: number) => void
  onSelect: (command: T) => void
}) {
  if (!state || commands.length === 0) return null
  return createPortal(
    <div
      data-slot="slash-menu"
      id={menuId}
      className="fixed z-[80] w-80 overflow-hidden rounded-lg bg-popover text-popover-foreground shadow-xl ring-1 ring-foreground/10"
      style={{ left: state.left, top: state.top }}
      role="listbox"
      aria-label="Editor block commands"
    >
      <div className="border-b px-3 py-2 text-[10px] font-semibold uppercase tracking-[.08em] text-muted-foreground">
        Basic blocks
      </div>
      <div className="max-h-80 overflow-y-auto p-1.5">
        {commands.map((command, itemIndex) => (
          <button
            id={`${menuId}-option-${itemIndex}`}
            type="button"
            role="option"
            aria-selected={index === itemIndex}
            className={cn(
              "flex w-full items-center gap-3 rounded-md px-2 py-2 text-left outline-none",
              index === itemIndex && "bg-accent text-accent-foreground",
            )}
            key={command.label}
            onMouseEnter={() => onIndex(itemIndex)}
            onMouseDown={(event) => {
              event.preventDefault()
              onSelect(command)
            }}
          >
            <span className="grid size-9 shrink-0 place-items-center rounded-md border bg-background">
              <command.icon className="size-4 text-muted-foreground" />
            </span>
            <span className="min-w-0">
              <span className="block text-sm font-medium">{command.label}</span>
              <span className="block truncate text-xs text-muted-foreground">
                {command.description}
              </span>
            </span>
          </button>
        ))}
      </div>
    </div>,
    document.body,
  )
}
