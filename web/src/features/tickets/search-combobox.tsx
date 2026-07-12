import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  useComboboxAnchor,
} from "../../components/ui/combobox"

export type SearchOption = { value: string; label: string; detail?: string }

type Props = {
  value: string
  options: SearchOption[]
  placeholder: string
  empty?: string
  contentClassName?: string
  onChange: (value: string) => void
}

export function searchOptionMatches(option: SearchOption, query: string) {
  const normalized = query.trim().toLocaleLowerCase()
  if (!normalized) return true
  return [option.label, option.value, option.detail].some((part) =>
    part?.toLocaleLowerCase().includes(normalized),
  )
}

export function SearchCombobox({
  value,
  options,
  placeholder,
  empty = "No matches.",
  contentClassName,
  onChange,
}: Props) {
  const selected = options.find((option) => option.value === value) ?? null
  const anchor = useComboboxAnchor()

  return (
    <Combobox
      items={options}
      value={selected}
      autoHighlight
      filter={searchOptionMatches}
      itemToStringLabel={(item) => item.label}
      itemToStringValue={(item) => item.value}
      onValueChange={(next) => onChange(next?.value ?? options[0]?.value ?? "")}
    >
      <div ref={anchor} className="w-full">
        <ComboboxInput
          className="w-full"
          aria-label={placeholder}
          placeholder={placeholder}
          showClear
        />
      </div>
      <ComboboxContent anchor={anchor} align="end" className={contentClassName}>
        <ComboboxList>
          {(option: SearchOption) => (
            <ComboboxItem key={option.value} value={option} className="py-2">
              <span className="min-w-0 flex-1 truncate">{option.label}</span>
              {option.detail && (
                <span className="max-w-20 shrink-0 truncate font-mono text-[10px] text-muted-foreground">
                  {option.detail}
                </span>
              )}
            </ComboboxItem>
          )}
        </ComboboxList>
        <ComboboxEmpty>{empty}</ComboboxEmpty>
      </ComboboxContent>
    </Combobox>
  )
}
