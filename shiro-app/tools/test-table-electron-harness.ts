// Electron（真实 Chromium）表格集成测试页：加载即跑场景，结果写入 window.__result
// 由 tools/test-table-electron.mjs 打包驱动（jsdom 的焦点/选区行为与真内核差异大，不可用）
import { EditorView } from '@codemirror/view'
import { EditorState } from '@codemirror/state'
import { history } from '@codemirror/commands'
import { tableEditing, insertTable, TABLE_MENU_EVENT } from '../src/utils/tableView'
import { livePreview } from '../src/utils/livePreview'

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function run(): Promise<void> {
  const failures: string[] = []
  const log: string[] = []
  const ok = (cond: boolean, msg: string) => {
    if (!cond) failures.push(msg)
    log.push(`${cond ? 'PASS' : 'FAIL'} ${msg}`)
  }
  try {
    const parent = document.createElement('div')
    document.body.appendChild(parent)
    const view = new EditorView({
      state: EditorState.create({ doc: '', extensions: [history(), tableEditing(), livePreview()] }),
      parent,
    })
    await sleep(30)

    // 场景 1：冷启动插入（编辑器未聚焦）——表格渲染、首格激活、焦点入格、CM 选区落在格内
    insertTable(view)
    await sleep(50)
    ok(view.state.doc.toString().startsWith('| 列一 |'), '插入模板文本')
    ok(view.dom.querySelectorAll('.md-table').length === 1, '表格挂件渲染')
    const cell0 = view.dom.querySelector('.md-cell-active') as HTMLElement | null
    ok(cell0?.textContent === '列一', `首个表头格激活（实际 ${cell0?.textContent ?? '无'}）`)
    ok(document.activeElement === cell0, '焦点在活动格内')
    ok(view.state.selection.main.from === 2 && view.state.selection.main.to === 4, `CM 选区在格内（实际 ${view.state.selection.main.from}-${view.state.selection.main.to}）`)

    // 场景 2：输入同步 + 挂件 DOM 复用（同一元素引用，焦点不丢）
    const cellBefore = view.dom.querySelector('.md-cell-active') as HTMLElement
    cellBefore.textContent = '列X'
    cellBefore.dispatchEvent(new Event('input', { bubbles: true }))
    await sleep(30)
    ok(view.state.doc.line(1).text.includes('列X'), '输入同步回文档')
    const cellAfter = view.dom.querySelector('.md-cell-active') as HTMLElement | null
    ok(cellAfter === cellBefore, '输入中挂件 DOM 复用（焦点不丢）')
    ok(document.activeElement === cellBefore, '输入后焦点仍在格内')

    // 场景 3：单元格内输入 |（转义写回，表格不坏）
    cellAfter!.textContent = '列X|Y'
    cellAfter!.dispatchEvent(new Event('input', { bubbles: true }))
    await sleep(30)
    ok(view.state.doc.line(1).text.includes('列X\\|Y'), `管道转义写回（实际 ${view.state.doc.line(1).text}）`)
    ok(view.dom.querySelectorAll('.md-table').length === 1, '表格结构未坏')

    // 场景 4：Tab 跳格
    cellAfter!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }))
    await sleep(30)
    const cellTab = view.dom.querySelector('.md-cell-active') as HTMLElement | null
    ok(cellTab?.dataset.c === '1', `Tab 到下一格（实际 c=${cellTab?.dataset.c}）`)
    ok(document.activeElement === cellTab, 'Tab 后焦点在新格内')

    // 场景 5/6：加行 / 加列按钮
    const rows0 = view.dom.querySelectorAll('.md-table tbody tr').length
    ;(view.dom.querySelector('.md-table-addrow') as HTMLElement).click()
    await sleep(30)
    ok(view.dom.querySelectorAll('.md-table tbody tr').length === rows0 + 1, '下缘 + 加行')
    const newRow = view.dom.querySelector('.md-table tbody tr:last-child') as HTMLElement
    ok(!!newRow.querySelector('td')?.querySelector('br'), '空单元格含 <br> 占位')
    ok(newRow.offsetHeight > 10, `新行有实际高度（实际 ${newRow.offsetHeight}px）`)
    const cols0 = view.dom.querySelectorAll('.md-table thead th').length
    ;(view.dom.querySelector('.md-table-addcol') as HTMLElement).click()
    await sleep(30)
    ok(view.dom.querySelectorAll('.md-table thead th').length === cols0 + 1, '右缘 + 加列')

    // 场景 7：右键菜单桥接
    let menuDetail: { items: { label?: string; action?: () => void }[] } | null = null
    view.dom.addEventListener(TABLE_MENU_EVENT, (e) => (menuDetail = (e as CustomEvent).detail))
    const bodyCell = view.dom.querySelector('.md-table tbody td') as HTMLElement
    bodyCell.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 10, clientY: 10 }))
    ok(!!menuDetail && menuDetail.items.some((i) => i.label === '删除行'), '右键菜单含行操作')
    ok(!!menuDetail && menuDetail.items.some((i) => i.label === '居中'), '右键菜单含对齐操作')

    // 场景 8：菜单动作（删除行）
    const rowsBefore = view.dom.querySelectorAll('.md-table tbody tr').length
    menuDetail!.items.find((i) => i.label === '删除行')!.action!()
    await sleep(30)
    ok(view.dom.querySelectorAll('.md-table tbody tr').length === rowsBefore - 1, '菜单删除行生效')

    // 场景 9：Escape 退出（光标落到表格后），再点表外触发自动对齐
    const activeNow = view.dom.querySelector('.md-cell-active') as HTMLElement | null
    activeNow?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    view.dispatch({ selection: { anchor: view.state.doc.length } })
    await sleep(30)
    ok(!view.dom.querySelector('.md-cell-active'), 'Escape 退出编辑')
    log.push('对齐后首行: ' + view.state.doc.line(1).text)

    ;(window as unknown as { __result: unknown }).__result = { failures, log }
  } catch (e) {
    ;(window as unknown as { __result: unknown }).__result = { failures: ['异常: ' + String(e)], log }
  }
}

void run()
