import { useMemo, useState } from "react"
import { Plus } from "lucide-react"
import {
  Combobox,
  ComboboxChip,
  ComboboxChips,
  ComboboxChipsInput,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxItem,
  ComboboxList,
  useComboboxAnchor,
} from "../../components/ui/combobox"
import { cn } from "../../lib/utils"

type Props = {
  value: string[]
  options: string[]
  contentClassName?: string
  onChange: (value: string[]) => void
}

export function labelOptions(options: string[], selected: string[]) {
  const unique = new Map<string, string>()
  for (const label of [...selected, ...options]) {
    const trimmed = label.trim()
    if (trimmed && !unique.has(trimmed.toLocaleLowerCase()))
      unique.set(trimmed.toLocaleLowerCase(), trimmed)
  }
  return [...unique.values()].sort((a, b) => a.localeCompare(b))
}

export function labelItems(options: string[], query: string) {
  const trimmed = query.trim()
  if (
    !trimmed ||
    options.some((option) => option.toLocaleLowerCase() === trimmed.toLocaleLowerCase())
  )
    return options
  return [...options, trimmed]
}

export function LabelCombobox({ value, options, contentClassName, onChange }: Props) {
  const [query, setQuery] = useState("")
  const anchor = useComboboxAnchor()
  const available = useMemo(() => labelOptions(options, value), [options, value])
  const items = useMemo(() => labelItems(available, query), [available, query])
  const creatable =
    query.trim() && items.at(-1) === query.trim() && !available.includes(query.trim())

  return (
    <Combobox
      items={items}
      multiple
      value={value}
      inputValue={query}
      autoHighlight
      onInputValueChange={setQuery}
      onValueChange={(next) => {
        onChange(next)
        setQuery("")
      }}
    >
      <ComboboxChips ref={anchor} className="min-h-9 w-full cursor-text">
        {value.map((label) => (
          <ComboboxChip key={label} aria-label={label} removeLabel={"Remove " + label}>
            {label}
          </ComboboxChip>
        ))}
        <ComboboxChipsInput
          aria-label="Add label"
          placeholder={value.length ? "Add label…" : "Search or create labels…"}
        />
      </ComboboxChips>
      <ComboboxContent
        anchor={anchor}
        align="end"
        className={cn(
          "w-[min(22rem,var(--available-width))] max-w-[calc(100vw-2rem)]",
          contentClassName,
        )}
      >
        <ComboboxList>
          {(label: string) => {
            const isCreate = Boolean(creatable) && label === query.trim()
            return (
              <ComboboxItem
                key={label}
                value={label}
                className={isCreate ? "border-t border-border/70 py-2" : "py-2"}
              >
                {isCreate ? (
                  <>
                    <Plus className="text-muted-foreground" />
                    <span className="min-w-0 truncate">
                      Create <span className="font-medium text-foreground">“{label}”</span>
                    </span>
                  </>
                ) : (
                  <span className="min-w-0 flex-1 truncate">{label}</span>
                )}
              </ComboboxItem>
            )
          }}
        </ComboboxList>
        <ComboboxEmpty>
          {query.trim() ? "No matching labels." : "No labels in this workspace."}
        </ComboboxEmpty>
      </ComboboxContent>
    </Combobox>
  )
}
