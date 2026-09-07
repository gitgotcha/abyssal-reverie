// Batch 0 prototype behavior checks. DOM checks do not replace visual QA.
const { JSDOM } = require('jsdom')
const fs = require('node:fs')
const path = require('node:path')
const assert = require('node:assert/strict')

function openPrototype() {
  const dom = new JSDOM(fs.readFileSync(path.join(__dirname, 'index.html'), 'utf8'), {
    runScripts: 'outside-only',
    url: 'http://localhost/',
  })
  const { window } = dom
  const document = window.document
  window.HTMLDialogElement.prototype.showModal = function () {
    this.open = true
  }
  window.HTMLDialogElement.prototype.close = function () {
    this.open = false
  }
  for (const file of ['assets/feather.min.js', 'assets/pinyin.js']) {
    window.eval(fs.readFileSync(path.join(__dirname, file), 'utf8'))
  }
  const enhancement = path.join(__dirname, 'batch0-enhancements.js')
  const appSource = fs.readFileSync(path.join(__dirname, 'app.js'), 'utf8')
  const enhancementSource = fs.existsSync(enhancement) ? fs.readFileSync(enhancement, 'utf8') : ''
  window.eval(`${appSource}\n${enhancementSource}\n;window.prototypeData=()=>data`)
  return { dom, window, document }
}

function click(document, selector) {
  const target = document.querySelector(selector)
  assert.ok(target, `missing ${selector}`)
  target.click()
}

function submit(window, document, selector) {
  const form = document.querySelector(selector)
  assert.ok(form, `missing ${selector}`)
  form.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true }))
}

const checks = [
  [
    'custom date picker offers quick selection and saves the local date',
    () => {
      const { dom, document, window } = openPrototype()
      try {
        click(document, '#deadline')
        assert.ok(document.querySelector('#date-popover.open'), 'custom date picker did not open')
        click(document, '[data-date-quick="tomorrow"]')
        const expected = new Date()
        expected.setDate(expected.getDate() + 1)
        const literal = `${expected.getFullYear()}-${String(expected.getMonth() + 1).padStart(2, '0')}-${String(expected.getDate()).padStart(2, '0')}`
        assert.equal(window.prototypeData().tasks.find((task) => task.id === 't1').date, literal)
        assert.equal(document.activeElement?.id, 'deadline')
      } finally {
        dom.window.close()
      }
    },
  ],
  [
    'simulated save failure keeps the draft and retry submits once',
    () => {
      const { dom, document, window } = openPrototype()
      try {
        click(document, '[data-nav="settings"]')
        click(document, '[data-action="simulate-save-failure"]')
        click(document, '[data-nav="management"]')
        click(document, '[data-action="create"]')
        const name = document.querySelector('[name="name"]')
        name.value = '保留草稿的任务'
        name.dispatchEvent(new window.Event('input', { bubbles: true }))
        const before = window.prototypeData().tasks.length
        submit(window, document, '#edit-form')
        assert.equal(window.prototypeData().tasks.length, before)
        assert.equal(document.querySelector('[name="name"]').value, '保留草稿的任务')
        assert.match(document.querySelector('#form-error').textContent, /保存失败/)
        click(document, '[data-action="retry-save"]')
        assert.equal(window.prototypeData().tasks.length, before + 1)
        assert.equal(document.querySelector('#modal').open, false)
      } finally {
        dom.window.close()
      }
    },
  ],
  [
    'undoing tag deletion never overwrites a later tag choice',
    () => {
      const { dom, document, window } = openPrototype()
      try {
        click(document, '[data-category="tags"]')
        click(document, '[data-action="delete-tag"]')
        click(document, '#confirm-dialog')
        click(document, '[data-category="tasks"]')
        click(document, '[data-task="t1"]')
        click(document, '[data-action="pick-tag"]')
        click(document, '[data-pick="g2"]')
        click(document, '[data-action="undo-tag-delete"]')
        const state = window.prototypeData()
        assert.ok(state.tags.some((tag) => tag.id === 'g1'), 'deleted tag entity was not restored')
        assert.equal(state.tasks.find((task) => task.id === 't1').tag, 'g2')
        assert.match(document.querySelector('#toast').textContent, /未覆盖后续修改/)
      } finally {
        dom.window.close()
      }
    },
  ],
]

let failed = 0
for (const [name, check] of checks) {
  try {
    check()
    console.log(`PASS ${name}`)
  } catch (error) {
    failed += 1
    console.error(`FAIL ${name}`)
    console.error(error.stack || error)
  }
}
console.log(`${checks.length - failed} passed, ${failed} failed`)
process.exit(failed ? 1 : 0)
