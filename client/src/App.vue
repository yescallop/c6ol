<script setup lang="ts">
import BoardView from './components/BoardView.vue';
</script>

<template>
  <BoardView />
</template>

<style>
body {
  /* Remove the default 8px margin from body. */
  margin: 0;
  background-color: #ffcc66;
}

/* Use `svh` to prevent overflow on mobile due to the hidable address bar. */
@supports (height: 100svh) {
  #app {
    height: 100svh;
  }
}

/* Old browsers might not support `svh`. */
@supports not (height: 100svh) {
  #app {
    height: 100vh;
  }
}
</style>