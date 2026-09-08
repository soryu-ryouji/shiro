<script setup lang="ts">
// Chat 视图：左列项目选择 + 会话列表，右列消息流 + 输入区。
// 对话修改项目文档：提案以卡片渲染，应用后整文件覆盖写盘（草稿→确认，见 docs/frontend/chat.md）。
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import {
  chatStore,
  stripProposalBlocks,
  type Proposal,
} from '../stores/chat'
import { renderMarkdown } from '../utils/mdRender'
import { editorParaMode } from '../utils/font'
import Icon from '../components/Icon.vue'
import SearchSelect from '../components/SearchSelect.vue'

onMounted(() => {
  void chatStore.loadProjects()
})

// ---- 项目选择 ----
const projectOptions = computed(() => chatStore.projects.map((p) => p.name))
const projectText = computed({
  get: () => chatStore.projectName,
  set: (v: string) => {
    const hit = chatStore.projects.find((p) => p.name === v)
    if (hit) void chatStore.selectProject(hit.path)
  },
})
// ---- 会话 ----
function fmtTime(ts: number): string {
  const d = new Date(ts * 1000)
  const today = new Date()
  const sameDay = d.toDateString() === today.toDateString()
  return sameDay
    ? d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
    : d.toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' })
}

// ---- 消息渲染 ----
function html(text: string): string {
  return renderMarkdown(stripProposalBlocks(text), editorParaMode.value)
}

// ---- 输入区 ----
const input = ref('')
const attachments = ref<string[]>([])
const attachText = ref('')
const inputEl = ref<HTMLTextAreaElement | null>(null)

const canSend = computed(() => input.value.trim() !== '' && !!chatStore.session && !chatStore.streaming)

/** 附件候选：未添加的文稿路径 */
const attachOptions = computed(() => chatStore.files.filter((f) => !attachments.value.includes(f)))

// SearchSelect 输入过程也发 update 事件：只在值与某个文件路径精确匹配时（即选中/完整输入）才加为附件
watch(attachText, (v) => {
  if (v && chatStore.files.includes(v) && !attachments.value.includes(v)) {
    attachments.value.push(v)
    attachText.value = ''
  }
})

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault()
    void submit()
  }
}

async function submit() {
  if (!canSend.value) return
  const content = input.value
  const files = [...attachments.value]
  input.value = ''
  attachments.value = []
  await chatStore.send(content, files)
  inputEl.value?.focus()
}

// 流式时消息区滚到底
const msgsEl = ref<HTMLDivElement | null>(null)
watch(
  () => [chatStore.session?.messages.length, chatStore.streamText] as const,
  async () => {
    await nextTick()
    const el = msgsEl.value
    if (el) el.scrollTop = el.scrollHeight
  },
)
</script>

<template>
  <section class="chat-view">
    <!-- 左列：项目 + 会话列表 -->
    <aside class="side">
      <div class="side-block">
        <div class="side-label">项目</div>
        <SearchSelect
          v-model="projectText"
          :options="projectOptions"
          strict
          placeholder="选择项目"
        />
      </div>

      <div class="side-block grow">
        <div class="side-label row">
          <span>会话</span>
          <button
            class="icon-btn"
            title="新建会话"
            :disabled="!chatStore.projectPath"
            @click="chatStore.newSession()"
          >
            <Icon name="plus" :size="14" />
          </button>
        </div>
        <div v-overlay-scrollbar class="session-list">
          <p v-if="!chatStore.projectPath" class="hint">先选择项目</p>
          <p v-else-if="!chatStore.sessions.length" class="hint">还没有会话</p>
          <button
            v-for="s in chatStore.sessions"
            :key="s.id"
            class="session"
            :class="{ active: s.id === chatStore.session?.id }"
            @click="chatStore.openSession(s.id)"
          >
            <span class="s-title">{{ s.title }}</span>
            <span class="s-meta">{{ s.message_count }} 条 · {{ fmtTime(s.updated_at) }}</span>
            <span class="del icon-btn" title="删除会话" @click.stop="chatStore.removeSession(s.id)">
              <Icon name="close" :size="12" />
            </span>
          </button>
        </div>
      </div>
    </aside>

    <!-- 右列：消息流 + 输入 -->
    <div class="main">
      <template v-if="chatStore.session">
        <div ref="msgsEl" v-overlay-scrollbar class="msgs">
          <div v-for="(m, i) in chatStore.session.messages" :key="i" class="msg" :class="m.role">
            <div class="who">{{ m.role === 'user' ? '我' : 'AI' }}</div>
            <div class="bubble">
              <!-- 引用文件 chips -->
              <div v-if="m.attachments?.length" class="att-row">
                <span v-for="a in m.attachments" :key="a" class="chip">{{ a }}</span>
              </div>
              <!-- eslint-disable-next-line vue/no-v-html 内容为本机手稿，同编辑器预览策略 -->
              <div class="md" v-html="html(m.content)" />
              <!-- 提案卡片 -->
              <div v-if="m.proposals?.length" class="proposals">
                <div v-for="p in m.proposals" :key="p.id" class="proposal" :class="{ applied: p.applied }">
                  <div class="p-head">
                    <Icon name="fileText" :size="13" />
                    <span class="p-file">{{ p.file }}</span>
                    <span v-if="p.applied" class="p-badge">已应用</span>
                    <button v-else class="btn apply" @click="chatStore.applyProposal(p)">应用</button>
                  </div>
                  <details>
                    <summary>查看内容</summary>
                    <pre>{{ p.content }}</pre>
                  </details>
                </div>
              </div>
            </div>
          </div>

          <!-- 流式中的回复 -->
          <div v-if="chatStore.streaming" class="msg assistant">
            <div class="who">AI</div>
            <div class="bubble">
              <div v-if="chatStore.streamThinking" class="thinking">{{ chatStore.streamThinking }}</div>
              <!-- eslint-disable-next-line vue/no-v-html -->
              <div v-if="chatStore.streamText" class="md" v-html="html(chatStore.streamText)" />
              <div v-else-if="!chatStore.streamThinking" class="pending">思考中…</div>
            </div>
          </div>
        </div>

        <!-- 输入区 -->
        <div class="composer">
          <div v-if="attachments.length" class="att-chips">
            <span v-for="a in attachments" :key="a" class="chip removable" @click="attachments = attachments.filter((x) => x !== a)">
              {{ a }}
              <Icon name="close" :size="10" />
            </span>
          </div>
          <div class="input-row">
            <div class="attach">
              <SearchSelect
                v-model="attachText"
                :options="attachOptions"
                strict
                placeholder="引用文件…"
              />
            </div>
            <textarea
              ref="inputEl"
              v-model="input"
              class="input"
              rows="1"
              placeholder="描述要修改的内容，Enter 发送，Shift+Enter 换行"
              @keydown="onKeydown"
            />
            <button v-if="chatStore.streaming" class="btn stop" @click="chatStore.stop()">停止</button>
            <button v-else class="btn send" :disabled="!canSend" @click="submit">发送</button>
          </div>
          <p v-if="chatStore.error" class="hint error">{{ chatStore.error }}</p>
        </div>
      </template>

      <div v-else class="empty">
        <template v-if="chatStore.projectPath">
          <p class="hint">选择左侧会话，或</p>
          <button class="btn" @click="chatStore.newSession()">新建会话</button>
          <p class="hint dim">和 AI 对话修改项目文档：AI 给出修改提案，你确认后写入文件</p>
        </template>
        <p v-else class="hint">在左侧选择一个项目开始</p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.chat-view {
  height: 100%;
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
}

/* ---- 左列 ---- */
.side {
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 12px 10px;
  gap: 12px;
}

.side-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.side-block.grow {
  flex: 1;
  min-height: 0;
}

.side-label {
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
  letter-spacing: 0.5px;
}

.side-label.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  width: 24px;
  height: 24px;
}

@media (hover: hover) {
  .icon-btn:hover {
    background: var(--border);
    color: var(--text);
  }
}

.session-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.session {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: none;
  text-align: left;
  cursor: pointer;
  color: var(--text);
}

@media (hover: hover) {
  .session:hover {
    background: var(--border);
  }
  .session:hover .del {
    opacity: 1;
  }
}

.session.active {
  background: var(--accent-soft);
}

.s-title {
  font-size: calc(13px * var(--font-scale-ui));
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  padding-right: 20px;
}

.s-meta {
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
}

.del {
  position: absolute;
  top: 6px;
  right: 6px;
  width: 20px;
  height: 20px;
  opacity: 0;
}

/* ---- 右列 ---- */
.main {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.msgs {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 20px 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.msg {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-width: 860px;
}

.msg.user {
  align-self: flex-end;
}

.who {
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
}

.msg.user .who {
  text-align: right;
}

.bubble {
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 10px 14px;
  background: var(--bg);
  font-size: calc(14px * var(--font-scale-ui));
  line-height: 1.7;
}

.msg.user .bubble {
  background: var(--accent-soft);
}

.thinking {
  max-height: 120px;
  overflow-y: auto;
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
  border-left: 2px solid var(--border);
  padding-left: 10px;
  margin-bottom: 8px;
  white-space: pre-wrap;
}

.pending {
  color: var(--text-dim);
}

.att-row {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-bottom: 6px;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border: 1px solid var(--border);
  border-radius: 999px;
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
}

.chip.removable {
  cursor: pointer;
}

/* 提案卡片 */
.proposals {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 10px;
}

.proposal {
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}

.proposal.applied {
  opacity: 0.65;
}

.p-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  background: var(--border);
}

.p-file {
  flex: 1;
  min-width: 0;
  font-size: calc(12px * var(--font-scale-ui));
  font-family: var(--font-mono, monospace);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.p-badge {
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
}

.btn {
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  color: var(--text);
  font-size: calc(12px * var(--font-scale-ui));
  padding: 4px 12px;
  cursor: pointer;
}

.btn.apply {
  border-color: var(--accent);
  color: var(--accent);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.proposal details {
  padding: 0 10px;
}

.proposal summary {
  padding: 6px 0;
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
  cursor: pointer;
  user-select: none;
}

.proposal pre {
  margin: 0 0 10px;
  padding: 10px;
  max-height: 280px;
  overflow: auto;
  background: var(--border);
  border-radius: 6px;
  font-size: calc(12px * var(--font-scale-ui));
  white-space: pre-wrap;
  word-break: break-all;
}

/* ---- 输入区 ---- */
.composer {
  border-top: 1px solid var(--border);
  padding: 12px 24px 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.att-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.input-row {
  display: flex;
  align-items: flex-end;
  gap: 8px;
}

.attach {
  width: 220px;
  flex: none;
}

.input {
  flex: 1;
  resize: none;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
  color: var(--text);
  font-family: inherit;
  font-size: calc(13px * var(--font-scale-ui));
  padding: 8px 12px;
  line-height: 1.5;
  max-height: 160px;
}

.input:focus {
  outline: none;
  border-color: var(--accent);
}

.btn.send {
  border-color: var(--accent);
  color: var(--accent);
}

.btn.stop {
  border-color: #d05050;
  color: #d05050;
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

.hint.dim {
  opacity: 0.7;
}

.hint.error {
  color: #d05050;
}

.empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
}
</style>
