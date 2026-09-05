<script setup lang="ts">
// 设置面板壳：左侧导航 + 右侧分区的两栏结构。交互要点：
// - 遮罩「按下与抬起都落在遮罩上」才关闭：面板内拖动选择文本滑出面板松开，按 click.self 判定会误关
// - Esc 关闭
// - 打开期间挂 body.dialog-open 挂起窗口拖拽区
// 分区视觉：macOS 设置式——灰底、白色分组卡片、行式布局（标签左、控件右、行间细分隔线）。
import { onMounted, ref } from 'vue'
import { apiBase } from '../api'
import { hasShell } from '../platform'
import { useDialogMask } from '../composables/useDialog'
import {
  applyEditorFont,
  applyEditorFontSize,
  applyEditorLineHeight,
  applyEditorParaGap,
  applyUiFont,
  applyUiFontSize,
  currentEditorFontKey,
  currentEditorFontSize,
  currentEditorLineHeight,
  currentEditorParaGap,
  currentEditorParaMode,
  currentUiFontKey,
  currentUiFontSize,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  LINE_HEIGHT_MAX,
  LINE_HEIGHT_MIN,
  PARA_GAP_MAX,
  PARA_GAP_MIN,
  applyEditorParaMode,
  listSystemFonts,
  type ParaMode,
} from '../utils/font'
import pkg from '../../package.json'

const emit = defineEmits<{ close: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

const SECTIONS = [
  { key: 'appearance', label: '外观' },
  { key: 'general', label: '通用' },
  { key: 'lan', label: '局域网' },
] as const
type SectionKey = (typeof SECTIONS)[number]['key']
const section = ref<SectionKey>('appearance')

// 字体（界面/正文分设，实时生效；key 为内置选项 key 或系统字体 family 名）
const uiFontKey = ref(currentUiFontKey())
const editorFontKey = ref(currentEditorFontKey())
function selectUiFont(key: string) {
  applyUiFont(key)
  uiFontKey.value = currentUiFontKey()
}
function selectEditorFont(key: string) {
  applyEditorFont(key)
  editorFontKey.value = currentEditorFontKey()
}

// 字号：界面/正文各自的数值框（实时生效）
const uiFontSize = ref(currentUiFontSize())
const editorFontSize = ref(currentEditorFontSize())
function onUiSizeInput(e: Event) {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isNaN(v)) return
  applyUiFontSize(v)
  uiFontSize.value = currentUiFontSize()
}
function onEditorSizeInput(e: Event) {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isNaN(v)) return
  applyEditorFontSize(v)
  editorFontSize.value = currentEditorFontSize()
}

// 编辑区排版：行间距（行高倍数）与段间距（段落首行额外间距 px），实时生效
const editorLineHeight = ref(currentEditorLineHeight())
const editorParaGap = ref(currentEditorParaGap())
function onEditorLineHeightInput(e: Event) {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isNaN(v)) return
  applyEditorLineHeight(v)
  editorLineHeight.value = currentEditorLineHeight()
}
function onEditorParaGapInput(e: Event) {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isNaN(v)) return
  applyEditorParaGap(v)
  editorParaGap.value = currentEditorParaGap()
}

// 分段方式：决定哪些行算「新段落」（段落间距的生效范围），切换后编辑器即时重算
const editorParaMode = ref(currentEditorParaMode())
function selectParaMode(m: ParaMode) {
  applyEditorParaMode(m)
  editorParaMode.value = m
}

// 系统字体（Local Font Access API；桌面端可用。选项名统一用界面字体渲染——符号字体的字母码位不可读）
const systemFonts = ref<string[] | null>(null)
onMounted(async () => {
  try {
    systemFonts.value = await listSystemFonts()
  } catch {
    systemFonts.value = null
  }
})
</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="dialog">
      <nav class="dialog-nav">
        <button
          v-for="s in SECTIONS"
          :key="s.key"
          class="dialog-nav-item"
          :class="{ active: section === s.key }"
          @click="section = s.key"
        >
          {{ s.label }}
        </button>
      </nav>

      <div class="dialog-body">
        <section v-show="section === 'appearance'" class="pane">
          <div class="group">
            <div class="grow">
              <span class="glabel">界面字体</span>
              <div class="editor-font-row">
                <select class="select" :value="uiFontKey" @change="selectUiFont(($event.target as HTMLSelectElement).value)">
                  <option value="sans">系统无衬线</option>
                  <option value="serif">系统衬线</option>
                  <option value="lxgw">霞鹜文楷（内置）</option>
                  <optgroup v-if="hasShell && systemFonts?.length" label="系统字体">
                    <option v-for="f in systemFonts" :key="f" :value="f">{{ f }}</option>
                  </optgroup>
                </select>
                <input
                  type="number"
                  class="size-num"
                  :min="FONT_SIZE_MIN"
                  :max="FONT_SIZE_MAX"
                  :step="0.5"
                  :value="uiFontSize"
                  title="界面字号"
                  @change="onUiSizeInput"
                />
                <span class="size-unit">px</span>
              </div>
            </div>
            <div class="grow">
              <span class="glabel">正文字体</span>
              <div class="editor-font-row">
                <select class="select" :value="editorFontKey" @change="selectEditorFont(($event.target as HTMLSelectElement).value)">
                  <option value="lxgw">霞鹜文楷（内置）</option>
                  <option value="sans">系统无衬线</option>
                  <option value="serif">系统衬线</option>
                  <optgroup v-if="hasShell && systemFonts?.length" label="系统字体">
                    <option v-for="f in systemFonts" :key="f" :value="f">{{ f }}</option>
                  </optgroup>
                </select>
                <input
                  type="number"
                  class="size-num"
                  :min="FONT_SIZE_MIN"
                  :max="FONT_SIZE_MAX"
                  :step="0.5"
                  :value="editorFontSize"
                  title="正文字号"
                  @change="onEditorSizeInput"
                />
                <span class="size-unit">px</span>
              </div>
            </div>
            <div class="grow">
              <span class="glabel">分段方式</span>
              <div class="editor-font-row">
                <select
                  class="select"
                  :value="editorParaMode"
                  title="决定哪些行算新段落（段落间距的生效范围）"
                  @change="selectParaMode(($event.target as HTMLSelectElement).value as ParaMode)"
                >
                  <option value="enter">回车分段（每行一段）</option>
                  <option value="blank">空行分段（Markdown 式）</option>
                </select>
              </div>
            </div>
            <div class="grow">
              <span class="glabel">段内行距</span>
              <div class="editor-font-row">
                <input
                  type="number"
                  class="size-num"
                  :min="LINE_HEIGHT_MIN"
                  :max="LINE_HEIGHT_MAX"
                  :step="0.1"
                  :value="editorLineHeight"
                  title="一段文字自动折行后，行与行之间的距离"
                  @change="onEditorLineHeightInput"
                />
                <span class="size-unit">倍</span>
              </div>
            </div>
            <div class="grow">
              <span class="glabel">段落间距</span>
              <div class="editor-font-row">
                <input
                  type="number"
                  class="size-num"
                  :min="PARA_GAP_MIN"
                  :max="PARA_GAP_MAX"
                  :step="1"
                  :value="editorParaGap"
                  title="按回车换行的行与行之间的额外距离"
                  @change="onEditorParaGapInput"
                />
                <span class="size-unit">px</span>
              </div>
            </div>
          </div>
          <p class="pane-hint">
            段内行距：一段长文字自动折行后，同一段内行与行的间距，调小更紧凑。段落间距：新段落首行的额外距离，哪些行算新段落由「分段方式」决定。
          </p>
        </section>

        <section v-show="section === 'general'" class="pane">
          <div class="group">
            <div class="grow">
              <span class="glabel">版本</span>
              <span class="gvalue">shiro {{ pkg.version }}</span>
            </div>
            <div class="grow">
              <span class="glabel">后端</span>
              <span class="gvalue mono">{{ apiBase }}</span>
            </div>
          </div>
        </section>

        <section v-show="section === 'lan'" class="pane">
          <div class="group">
            <div class="grow">
              <span class="glabel">开启局域网访问</span>
              <input type="checkbox" disabled />
            </div>
          </div>
          <p class="pane-hint">
            开启后局域网内的设备（iPad、手机等）可通过浏览器访问 shiro。访问 key 的生成与管理将在后续版本提供（设计见
            docs/architecture.md「局域网访问」）。
          </p>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mask {
  position: fixed;
  inset: 0;
  z-index: 200;
  background: rgba(0, 0, 0, 0.25);
  display: flex;
  align-items: center;
  justify-content: center;
}

/* macOS 设置式：灰底窗口 + 白色分组卡片 */
.dialog {
  width: min(620px, 90vw);
  height: min(460px, 82vh);
  display: flex;
  border-radius: 12px;
  background: var(--bg-soft);
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2);
  overflow: hidden;
}

.dialog-nav {
  flex: none;
  width: 148px;
  padding: 14px 10px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.dialog-nav-item {
  padding: 6px 10px;
  border: none;
  border-radius: 6px;
  background: none;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  text-align: left;
  cursor: pointer;
}

@media (hover: hover) {
  .dialog-nav-item:hover {
    background: var(--border);
  }
}

.dialog-nav-item.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}

.dialog-body {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}

.pane {
  padding: 16px 18px;
}

/* 分组卡片：白底圆角，行式布局，行间细分隔线 */
.group {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}

.grow {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 9px 14px;
}

.grow + .grow {
  border-top: 1px solid var(--border);
}

.glabel {
  flex: none;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
}

.gvalue {
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text-dim);
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mono {
  font-family: ui-monospace, Consolas, monospace;
}

.pane-hint {
  margin: 10px 4px 0;
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
  line-height: 1.6;
}

/* 控件：下拉与字号 */
.select {
  appearance: none;
  max-width: 240px;
  height: 28px;
  padding: 0 26px 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background:
    var(--bg-soft)
    url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%23888' stroke-width='1.5'/%3E%3C/svg%3E")
    no-repeat right 8px center;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  cursor: pointer;
  outline: none;
}

.select:focus {
  border-color: var(--accent);
}

/* 字号：数值框（并入正文字体行右侧） */
.editor-font-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.size-num {
  width: 56px;
  height: 26px;
  padding: 0 6px;
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  outline: none;
}

.size-num:focus {
  border-color: var(--accent);
}

.size-unit {
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}
</style>
