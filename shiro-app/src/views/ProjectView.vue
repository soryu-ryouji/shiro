<template>
  <!-- 未打开项目时的中栏欢迎页；打开后中栏为编辑器（章节编辑待实现） -->
  <section class="welcome">
    <h1 class="logo">shiro</h1>
    <p class="hint">从左侧打开项目文件夹开始</p>
  </section>
</template>

<style scoped>
.welcome {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
}

.logo {
  margin: 0;
  font-size: 32px;
  font-weight: 700;
  letter-spacing: 1px;
  color: var(--text);
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: 13px;
}
</style>
