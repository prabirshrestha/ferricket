import { expect, test } from "bun:test"
import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { Command, CommandEmpty, CommandInput, CommandItem, CommandList } from "./command"

const item = { id: "one", label: "One" }

test("the mounted empty live region does not reserve space when results exist", () => {
  const markup = renderToStaticMarkup(
    createElement(
      Command,
      { items: [item] },
      createElement(CommandInput, { placeholder: "Search" }),
      createElement(
        CommandList,
        null,
        createElement(CommandEmpty, null, "No results"),
        createElement(CommandItem, { value: item }, "One"),
      ),
    ),
  )

  expect(markup).toContain('role="status"')
  expect(markup).not.toContain("px-3 py-10")
  expect(markup).toContain(">One</div>")
})

test("the empty message keeps its spacing when there are no results", () => {
  const markup = renderToStaticMarkup(
    createElement(
      Command,
      { items: [] },
      createElement(CommandInput, { placeholder: "Search" }),
      createElement(CommandList, null, createElement(CommandEmpty, null, "No results")),
    ),
  )

  expect(markup).toContain('<div class="px-3 py-10">No results</div>')
})
