/* Batch 0 prototype additions: local-date picker, save failure/retry, and guarded tag-delete undo. */
(() => {
  const two = (value) => String(value).padStart(2, '0')
  const toLocalDate = (date) => `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}`
  const fromLocalDate = (value) => {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value || '')
    if (!match) return null
    const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]))
    return toLocalDate(date) === value ? date : null
  }
  const shiftDays = (date, days) => {
    const shifted = new Date(date.getFullYear(), date.getMonth(), date.getDate())
    shifted.setDate(shifted.getDate() + days)
    return shifted
  }

  let activeDateInput = null
  let activeDateSelector = ''
  let visibleMonth = new Date(new Date().getFullYear(), new Date().getMonth(), 1)
  let cursorDate = new Date()
  let failNextSave = false
  let pendingTagDelete = null
  let tagDeleteTimer = null
  const tagRevisions = new Map(data.tasks.map((task) => [task.id, 0]))

  const popover = document.createElement('section')
  popover.id = 'date-popover'
  popover.className = 'date-popover'
  popover.setAttribute('role', 'dialog')
  popover.setAttribute('aria-label', '选择截止日期')
  document.body.append(popover)

  function inputSelector(input) {
    if (input.id) return `#${input.id}`
    return input.name ? `[name="${input.name}"]` : 'input[type="date"]'
  }

  function enhanceDateInputs(root = document) {
    root.querySelectorAll('input[type="date"]').forEach((input) => {
      input.readOnly = true
      input.setAttribute('aria-haspopup', 'dialog')
      input.title = '打开日期选择器；弹层中仍可手动输入 YYYY-MM-DD'
    })
  }

  function paintCalendar() {
    const selectedValue = activeDateInput?.value || ''
    const monthStart = new Date(visibleMonth.getFullYear(), visibleMonth.getMonth(), 1)
    const nextMonth = new Date(visibleMonth.getFullYear(), visibleMonth.getMonth() + 1, 1)
    const daysInMonth = Math.round((nextMonth - monthStart) / 86400000)
    const leading = (monthStart.getDay() + 6) % 7
    const today = toLocalDate(new Date())
    const cells = []
    for (let index = 0; index < leading; index += 1) cells.push('<span class="date-empty"></span>')
    for (let day = 1; day <= daysInMonth; day += 1) {
      const date = new Date(visibleMonth.getFullYear(), visibleMonth.getMonth(), day)
      const value = toLocalDate(date)
      const flags = [value === today ? 'today' : '', value === selectedValue ? 'selected' : '', value === toLocalDate(cursorDate) ? 'cursor' : ''].filter(Boolean).join(' ')
      cells.push(`<button type="button" class="date-cell ${flags}" data-date-value="${value}" aria-label="${value}" ${value === selectedValue ? 'aria-pressed="true"' : ''}>${day}</button>`)
    }
    popover.innerHTML = `<div class="date-head"><button type="button" data-date-nav="previous" aria-label="上个月">${icon('chevron-left')}</button><strong>${visibleMonth.getFullYear()} 年 ${visibleMonth.getMonth() + 1} 月</strong><button type="button" data-date-nav="next" aria-label="下个月">${icon('chevron-right')}</button></div><div class="date-week" aria-hidden="true">${['一', '二', '三', '四', '五', '六', '日'].map((day) => `<span>${day}</span>`).join('')}</div><div class="date-grid">${cells.join('')}</div><div class="date-quick"><button type="button" data-date-quick="today">今天</button><button type="button" data-date-quick="tomorrow">明天</button><button type="button" data-date-quick="week">一周后</button><button type="button" data-date-quick="clear">清除</button></div><form id="manual-date-form" class="manual-date"><label for="manual-date">手动输入</label><div><input id="manual-date" inputmode="numeric" placeholder="YYYY-MM-DD" value="${selectedValue}"><button class="outline" type="submit">应用</button></div><small id="manual-date-error" class="error"></small></form>`
  }

  function openCalendar(input) {
    activeDateInput = input
    activeDateSelector = inputSelector(input)
    const initial = fromLocalDate(input.value) || new Date()
    visibleMonth = new Date(initial.getFullYear(), initial.getMonth(), 1)
    cursorDate = initial
    paintCalendar()
    popover.classList.add('open')
    popover.querySelector(`[data-date-value="${toLocalDate(cursorDate)}"]`)?.focus()
  }

  function closeCalendar() {
    popover.classList.remove('open')
    const trigger = document.querySelector(activeDateSelector)
    activeDateInput = null
    trigger?.focus()
  }

  function chooseDate(value) {
    if (!activeDateInput) return
    const selector = activeDateSelector
    activeDateInput.value = value
    activeDateInput.dispatchEvent(new Event('input', { bubbles: true }))
    activeDateInput.dispatchEvent(new Event('change', { bubbles: true }))
    popover.classList.remove('open')
    activeDateInput = null
    document.querySelector(selector)?.focus()
  }

  function moveCursor(days) {
    cursorDate = shiftDays(cursorDate, days)
    visibleMonth = new Date(cursorDate.getFullYear(), cursorDate.getMonth(), 1)
    paintCalendar()
    popover.querySelector(`[data-date-value="${toLocalDate(cursorDate)}"]`)?.focus()
  }

  function showTagUndo() {
    document.querySelector('#tag-undo-banner')?.remove()
    const banner = document.createElement('aside')
    banner.id = 'tag-undo-banner'
    banner.className = 'tag-undo-banner'
    banner.innerHTML = '<span>标签已删除。撤销只恢复此后没有重新选择标签的任务。</span><button class="outline" data-action="undo-tag-delete">撤销删除标签</button>'
    document.body.append(banner)
    clearTimeout(tagDeleteTimer)
    tagDeleteTimer = setTimeout(() => {
      pendingTagDelete = null
      banner.remove()
    }, 15000)
  }

  function deleteTagWithGuard(tag) {
    const index = data.tags.indexOf(tag)
    const snapshots = data.tasks.filter((task) => task.tag === tag.id).map((task) => {
      const clearedRevision = (tagRevisions.get(task.id) || 0) + 1
      tagRevisions.set(task.id, clearedRevision)
      task.tag = ''
      return { taskId: task.id, oldTag: tag.id, clearedRevision }
    })
    data.tags.splice(index, 1)
    selected.tags = null
    pendingTagDelete = { tag, index, snapshots }
    render()
    notify('标签已删除，任务与投入记录保留')
    showTagUndo()
  }

  function undoTagDelete() {
    if (!pendingTagDelete) return
    const { tag, index, snapshots } = pendingTagDelete
    if (!data.tags.some((item) => item.id === tag.id)) data.tags.splice(index, 0, tag)
    let skipped = 0
    for (const snapshot of snapshots) {
      const task = find('tasks', snapshot.taskId)
      if (task && task.tag === '' && tagRevisions.get(task.id) === snapshot.clearedRevision) {
        task.tag = snapshot.oldTag
        tagRevisions.set(task.id, snapshot.clearedRevision + 1)
      } else {
        skipped += 1
      }
    }
    pendingTagDelete = null
    clearTimeout(tagDeleteTimer)
    document.querySelector('#tag-undo-banner')?.remove()
    render()
    notify(skipped ? `标签已恢复；${skipped} 个任务未覆盖后续修改` : '标签与原关联已恢复')
  }

  const originalPicker = picker
  picker = function guardedPicker(kind) {
    const taskId = selected.tasks
    originalPicker(kind)
    if (kind !== 'tags') return
    document.querySelectorAll('[data-pick]').forEach((button) => {
      button.addEventListener('click', () => tagRevisions.set(taskId, (tagRevisions.get(taskId) || 0) + 1))
    })
    document.querySelector('#picker-new')?.addEventListener('click', () => {
      tagRevisions.set(taskId, (tagRevisions.get(taskId) || 0) + 1)
    })
  }

  const originalSettings = settings
  settings = function enhancedSettings() {
    originalSettings()
    const resetButton = document.querySelector('[data-action="reset-demo"]')
    resetButton?.insertAdjacentHTML('beforebegin', '<h2>失败恢复演示</h2><p>点击后，下一次新建或编辑会模拟一次保存失败；草稿保留，可原位重试。</p><button class="outline" data-action="simulate-save-failure" style="margin-top:12px">模拟下次保存失败</button><br>')
  }

  document.addEventListener('click', (event) => {
    const dateInput = event.target.closest?.('input[type="date"]')
    if (dateInput) {
      event.preventDefault()
      openCalendar(dateInput)
      return
    }
    const button = event.target.closest?.('button')
    if (!button) return
    if (button.dataset.dateNav) {
      const delta = button.dataset.dateNav === 'previous' ? -1 : 1
      visibleMonth = new Date(visibleMonth.getFullYear(), visibleMonth.getMonth() + delta, 1)
      cursorDate = new Date(visibleMonth)
      paintCalendar()
      return
    }
    if (button.dataset.dateValue) return chooseDate(button.dataset.dateValue)
    if (button.dataset.dateQuick) {
      const today = new Date()
      const offsets = { today: 0, tomorrow: 1, week: 7 }
      return chooseDate(button.dataset.dateQuick === 'clear' ? '' : toLocalDate(shiftDays(today, offsets[button.dataset.dateQuick])))
    }
    if (button.dataset.action === 'simulate-save-failure') {
      failNextSave = true
      notify('已准备：下一次保存会失败一次，随后可重试')
      return
    }
    if (button.dataset.action === 'retry-save') {
      event.preventDefault()
      event.stopImmediatePropagation()
      document.querySelector('#edit-form')?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
      return
    }
    if (button.dataset.action === 'delete-tag') {
      event.preventDefault()
      event.stopImmediatePropagation()
      const tag = find('tags', selected.tags)
      const affected = data.tasks.filter((task) => task.tag === tag.id).length
      confirmAction(`删除标签“${tag.name}”？`, `${affected} 个任务会解除此标签，任务及投入记录都会保留。`, () => deleteTagWithGuard(tag))
      return
    }
    if (button.dataset.action === 'undo-tag-delete') {
      event.preventDefault()
      event.stopImmediatePropagation()
      undoTagDelete()
    }
  }, true)

  document.addEventListener('submit', (event) => {
    if (event.target.id === 'manual-date-form') {
      event.preventDefault()
      event.stopImmediatePropagation()
      const value = document.querySelector('#manual-date').value.trim()
      if (!fromLocalDate(value)) {
        document.querySelector('#manual-date-error').textContent = '请输入真实有效的 YYYY-MM-DD 日期。'
        return
      }
      chooseDate(value)
      return
    }
    if (event.target.id === 'edit-form' && failNextSave) {
      event.preventDefault()
      event.stopImmediatePropagation()
      failNextSave = false
      document.querySelector('#form-error').innerHTML = '保存失败，内容与选择均已保留。<button type="button" class="outline retry-save" data-action="retry-save">重试保存</button>'
    }
  }, true)

  document.addEventListener('keydown', (event) => {
    if (!popover.classList.contains('open')) return
    if (event.key === 'Escape') {
      event.preventDefault()
      closeCalendar()
    } else if (event.key === 'ArrowLeft') {
      event.preventDefault()
      moveCursor(-1)
    } else if (event.key === 'ArrowRight') {
      event.preventDefault()
      moveCursor(1)
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      moveCursor(-7)
    } else if (event.key === 'ArrowDown') {
      event.preventDefault()
      moveCursor(7)
    } else if (event.key === 'Enter' && event.target.matches('[data-date-value]')) {
      event.preventDefault()
      chooseDate(event.target.dataset.dateValue)
    }
  })

  const observer = new MutationObserver(() => enhanceDateInputs())
  observer.observe(document.body, { childList: true, subtree: true })
  enhanceDateInputs()
})()
