<script setup lang="ts">
import Board from './components/Board.vue';
</script>

<template>
  <Board />
</template>

<style>
body {
  /* Remove the default 8px margin from body. */
  margin: 0;
  background-color: #ffcc66;
}

#app {
  /* Use `svh` to prevent overflow on mobile due to the hidable address bar. */
  height: 100svh;
}
</style>