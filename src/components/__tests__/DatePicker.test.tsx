import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"
import { DatePicker } from "../DatePicker"
import { addLocalDays, todayLocalDate } from "../../domain/localDate"

afterEach(() => cleanup())

describe("DatePicker", () => {
  it("opens a calendar and selects a local date", async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<DatePicker value="" onChange={onChange} />)

    await user.click(screen.getByRole("button", { name: "截止日期日历" }))
    expect(screen.getByRole("dialog", { name: "截止日期选择器" })).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: todayLocalDate() }))

    expect(onChange).toHaveBeenCalledWith(todayLocalDate())
    expect(screen.queryByRole("dialog", { name: "截止日期选择器" })).not.toBeInTheDocument()
  })

  it("keeps precise text entry and rejects incomplete dates", async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<DatePicker value="" onChange={onChange} />)
    const input = screen.getByRole("textbox", { name: "截止日期" })

    await user.type(input, "2026-02-30")
    expect(onChange).not.toHaveBeenCalledWith("2026-02-30")
    expect(screen.getByRole("alert")).toHaveTextContent("请输入有效日期")
    await user.clear(input)
    await user.type(input, "2026-02-28")
    expect(onChange).toHaveBeenLastCalledWith("2026-02-28")
  })

  it("offers clear and today shortcuts", async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<DatePicker value="2026-02-28" onChange={onChange} />)
    await user.click(screen.getByRole("button", { name: "截止日期日历" }))
    await user.click(screen.getByRole("button", { name: "清除" }))
    expect(onChange).toHaveBeenCalledWith("")
  })

  it("supports arrow-key navigation and Enter selection", async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    const today = todayLocalDate()
    render(<DatePicker value={today} onChange={onChange} />)
    await user.click(screen.getByRole("button", { name: "截止日期日历" }))
    screen.getByRole("dialog", { name: "截止日期选择器" }).focus()
    await user.keyboard("{ArrowRight}{Enter}")
    expect(onChange).toHaveBeenCalledWith(addLocalDays(today, 1))
  })
})
