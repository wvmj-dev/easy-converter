<script setup lang="ts" generic="T extends string">
defineProps<{ legend: string; name: string; choices: { value: T; label: string }[] }>()
const value = defineModel<T>({ required: true })
</script>

<template>
  <fieldset class="field">
    <legend class="field__label">
      {{ legend }}
    </legend>
    <label
      v-for="choice in choices"
      :key="choice.value"
      class="radio"
    >
      <input
        v-model="value"
        type="radio"
        :name="name"
        :value="choice.value"
      >
      {{ choice.label }}
    </label>
  </fieldset>
</template>
