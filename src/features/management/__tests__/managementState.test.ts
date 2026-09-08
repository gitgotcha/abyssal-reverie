import { afterEach, describe, expect, it } from "vitest"
import { DEFAULT_MANAGEMENT_STATE, MANAGEMENT_STATE_KEY, loadManagementState, saveManagementState } from "../managementState"

afterEach(() => window.sessionStorage.clear())

describe("management state", () => {
  it("restores the selected view and task context between visits", () => {
    const next = {
      ...DEFAULT_MANAGEMENT_STATE,
      view: "projects" as const,
      taskSearch: "面试",
      taskStatusFilter: "all" as const,
      taskScrollTop: 240,
      returnContext: { view: "tasks" as const, taskId: "t-1" },
    }
    saveManagementState(next)
    expect(loadManagementState()).toEqual(next)
  })

  it("falls back safely when stored state is malformed", () => {
    window.sessionStorage.setItem(MANAGEMENT_STATE_KEY, "not-json")
    expect(loadManagementState()).toEqual(DEFAULT_MANAGEMENT_STATE)
  })
})
