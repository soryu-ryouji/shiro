// tableView.ts 解析/对齐逻辑冒烟测试（无前端测试框架，用 esbuild 打包后在 node 直跑）
// 用法：cd shiro-app && node tools/test-tableview.mjs
import { build } from 'esbuild'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import assert from 'node:assert/strict'

// 产物落在项目内临时目录，跑完即删（含断言失败路径）
const outDir = mkdtempSync(join(process.cwd(), 'tools', '.test-'))
const outfile = join(outDir, 'tableView.mjs')
try {
  await run()
} finally {
  rmSync(outDir, { recursive: true, force: true })
}

async function run() {
  await build({ entryPoints: ['src/utils/tableView.ts'], bundle: true, format: 'esm', outfile, logLevel: 'silent' })
  const { EditorState } = await import('@codemirror/state')
  const { scanTables, findTableAt, alignTableText, displayWidth, gridOf, renderGrid, gridInsertRow, gridDeleteRow, gridMoveRow, gridInsertCol, gridDeleteCol, gridMoveCol, gridSetAlign, escapeCellText, rawOffsetOfDecoded } = await import(pathToFileURL(outfile))

let state
const doc = (text) => (state = EditorState.create({ doc: text }).doc)

// 1. CJK 列宽对齐（中文按 2）
{
  const text = ['| 名称 | 说明 |', '| --- | :-: |', '| 剑 | short |', '| 很长的中文名 | x |'].join('\n')
  const blocks = scanTables(doc(text))
  assert.equal(blocks.length, 1)
  assert.equal(blocks[0].startLine, 1)
  assert.equal(blocks[0].endLine, 4)
  assert.deepEqual(blocks[0].aligns, ['none', 'center'])
  const aligned = alignTableText(doc(text), blocks[0])
  const expected = [
    '| 名称         | 说明  |',
    '| ------------ | :---: |',
    '| 剑           | short |',
    '| 很长的中文名 |   x   |',
  ].join('\n')
  assert.equal(aligned, expected)
  // 幂等：对齐后的块再对齐，结果不变
  assert.equal(alignTableText(doc(expected), scanTables(doc(expected))[0]), expected)
}

// 2. 参差不齐的行补齐，不丢数据
{
  const text = ['| a | b |', '| - | - |', '| 1 |', '| 1 | 2 | 3 |'].join('\n')
  const aligned = alignTableText(doc(text), scanTables(doc(text))[0])
  const expected = ['| a   | b   |     |', '| --- | --- | --- |', '| 1   |     |     |', '| 1   | 2   | 3   |'].join('\n')
  assert.equal(aligned, expected)
}

// 3. 非表格不误判：--- 分隔线、围栏内容、标题行、引用行
{
  const text = ['---', '', '```', '| a | b |', '| - | - |', '```', '', '# 标 | 题', '- | - |'].join('\n')
  assert.deepEqual(scanTables(doc(text)), [])
}

// 4. 表格在围栏后正常识别；光标定位
{
  const text = ['前言', '', '| a | b |', '| - | - |', '| 1 | 2 |', '', '后记'].join('\n')
  const blocks = scanTables(doc(text))
  assert.equal(blocks.length, 1)
  assert.equal(blocks[0].startLine, 3)
  assert.equal(blocks[0].endLine, 5)
  const state2 = EditorState.create({ doc: text, selection: { anchor: text.indexOf('2 |') } })
  assert.ok(findTableAt(state2))
  const state3 = EditorState.create({ doc: text, selection: { anchor: 0 } })
  assert.equal(findTableAt(state3), null)
}

// 5. 显示宽度
assert.equal(displayWidth('中文a'), 5)
assert.equal(displayWidth(''), 0)

// 6. 表格后紧跟含 | 的段落行会被吸入正文行（v1 已知取舍），但空行能截断
{
  const text = ['| a | b |', '| - | - |', '', '正文 | 里有管道'].join('\n')
  const blocks = scanTables(doc(text))
  assert.equal(blocks[0].endLine, 2)
}

// 7. 转义管道：\| 不作分隔符，解析解码为 |；行尾转义管道不被误当收尾
{
  const text = ['| a \\| b | c |', '| --- | --- |', '| 1 | 2 |'].join('\n')
  const blocks = scanTables(doc(text))
  assert.equal(blocks.length, 1)
  assert.deepEqual(blocks[0].header.cells.map((c) => c.text), ['a | b', 'c'])
  const text2 = ['| x \\|', '| - |'].join('\n')
  assert.deepEqual(scanTables(doc(text2))[0].header.cells.map((c) => c.text), ['x |'])
}

// 8. 网格模型与结构变换（纯函数）
{
  const text = ['| a | b |', '| - | :- |', '| 1 | 2 |', '| 3 | 4 |'].join('\n')
  const block = scanTables(doc(text))[0]
  const grid = gridOf(block)
  assert.deepEqual(grid, { aligns: ['none', 'left'], header: ['a', 'b'], rows: [['1', '2'], ['3', '4']] })

  assert.deepEqual(gridInsertRow(grid, 1).rows, [['1', '2'], ['', ''], ['3', '4']])
  assert.deepEqual(gridInsertRow(grid, 9).rows, [['1', '2'], ['3', '4'], ['', '']])
  assert.deepEqual(gridDeleteRow(grid, 0).rows, [['3', '4']])
  assert.deepEqual(gridMoveRow(grid, 0, 1).rows, [['3', '4'], ['1', '2']])
  assert.deepEqual(gridMoveRow(grid, 0, -1).rows, grid.rows) // 越界不变

  const g2 = gridInsertCol(grid, 1)
  assert.deepEqual(g2.header, ['a', '', 'b'])
  assert.deepEqual(g2.aligns, ['none', 'none', 'left'])
  assert.deepEqual(g2.rows, [['1', '', '2'], ['3', '', '4']])
  assert.deepEqual(gridDeleteCol(g2, 1).header, ['a', 'b'])
  assert.deepEqual(gridDeleteCol(grid, 0).aligns, ['left'])
  const g3 = gridMoveCol(grid, 0, 1)
  assert.deepEqual(g3.header, ['b', 'a'])
  assert.deepEqual(g3.aligns, ['left', 'none'])
  assert.deepEqual(g3.rows, [['2', '1'], ['4', '3']])
  assert.deepEqual(gridSetAlign(grid, 0, 'center').aligns, ['center', 'left'])
  // 原网格不被修改
  assert.deepEqual(grid.rows, [['1', '2'], ['3', '4']])
}

// 9. 渲染：转义写回 + 重解析往返一致 + 单元格定位正确
{
  const grid = { aligns: ['none', 'right'], header: ['x | y', 'b'], rows: [['1', '22']] }
  const { text, cells } = renderGrid(grid)
  const reparsed = scanTables(doc(text))[0]
  assert.equal(reparsed.header.cells[0].text, 'x | y')
  assert.deepEqual(reparsed.aligns, ['none', 'right'])
  assert.deepEqual(reparsed.rows[0].cells.map((c) => c.text), ['1', '22'])
  // cells 定位：表头第一格原文起点与长度（含转义）
  assert.equal(text.slice(cells[0][0].start, cells[0][0].start + cells[0][0].len), 'x \\| y')
  // 第二格右对齐：定位跳过前导空格
  assert.equal(text.slice(cells[0][1].start, cells[0][1].start + cells[0][1].len), 'b')
  // 幂等：渲染结果再对齐不变
  assert.equal(alignTableText(doc(text), reparsed), text)
}

// 10. 转义与偏移换算
assert.equal(escapeCellText('a|b'), 'a\\|b')
assert.equal(escapeCellText('abc'), 'abc')
assert.equal(rawOffsetOfDecoded('a\\|b', 0), 0)
assert.equal(rawOffsetOfDecoded('a\\|b', 2), 3) // 显示偏移 2（| 之后）→ 原文偏移 3（\| 之后）
assert.equal(rawOffsetOfDecoded('abc', 2), 2)

console.log('test-tableview: all passed')
}
