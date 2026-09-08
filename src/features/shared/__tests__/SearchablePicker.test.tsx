import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"
import { SearchablePicker } from "../SearchablePicker"

afterEach(() => cleanup())

describe("SearchablePicker", () => {
  it("searches options and keeps an explicit unset choice", async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<SearchablePicker value="" onChange={onChange} ariaLabel="所属项目" emptyLabel="独立任务"
      options={[{ value: "p1", label: "Java 面试", group: "工作" }, { value: "p2", label: "读书", group: "学习" }]} />)

    await user.click(screen.getByRole("combobox", { name: "所属项目" }))
    await user.type(screen.getByRole("combobox", { name: "所属项目" }), "java")
    expect(screen.getByRole("option", { name: /Java 面试/ })).toBeInTheDocument()
    expect(screen.queryByRole("option", { name: /读书/ })).not.toBeInTheDocument()
    await user.click(screen.getByRole("option", { name: /Java 面试/ }))
    expect(onChange).toHaveBeenCalledWith("p1")
  })

  it("clears an existing relationship instead of choosing a fallback", async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<SearchablePicker value="p1" onChange={onChange} ariaLabel="所属项目" emptyLabel="独立任务"
      options={[{ value: "p1", label: "Java 面试" }]} />)
    await user.click(screen.getByRole("combobox", { name: "所属项目" }))
    await user.click(screen.getByRole("option", { name: "独立任务" }))
    expect(onChange).toHaveBeenCalledWith("")
  })

  it("supports arrow-key choice and Enter confirmation", async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<SearchablePicker value="" onChange={onChange} ariaLabel="所属项目" emptyLabel="独立任务"
      options={[{ value: "p1", label: "Java 面试" }, { value: "p2", label: "读书" }]} />)

    const input = screen.getByRole("combobox", { name: "所属项目" })
    await user.click(input)
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}")
    expect(onChange).toHaveBeenCalledWith("p2")
  })
})
