<script setup lang="ts">
import { useI18n } from 'vue-i18n'

export interface OperationChoice {
  id: string
  label: string
  description: string
  /** Translated reason when the operation cannot be used with the current files. */
  unavailable?: string | null
}

defineProps<{ operations: OperationChoice[]; name: string }>()
const selected = defineModel<string>({ required: true })
const { t } = useI18n()
</script>

<template>
  <fieldset class="picker">
    <legend class="section-title picker__legend">
      {{ t('operations.title') }}
    </legend>
    <div class="picker__grid">
      <label
        v-for="op in operations"
        :key="op.id"
        class="picker__option"
        :class="{
          'picker__option--selected': selected === op.id,
          'picker__option--unavailable': !!op.unavailable,
        }"
      >
        <input
          v-model="selected"
          class="picker__input"
          type="radio"
          :name="name"
          :value="op.id"
        >
        <span class="picker__label">{{ op.label }}</span>
        <span class="picker__description">{{ op.unavailable || op.description }}</span>
      </label>
    </div>
  </fieldset>
</template>

<style scoped>
.picker {
  margin: 0;
  padding: 0;
  border: 0;
}

.picker__legend {
  margin-bottom: var(--space-2);
  padding: 0;
}

.picker__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
  gap: var(--space-2);
}

.picker__option {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg);
  cursor: pointer;
}

.picker__option:hover {
  border-color: var(--color-border-strong);
}

.picker__option--selected {
  border-color: var(--color-accent);
  background: var(--color-accent-soft);
}

.picker__option:has(.picker__input:focus-visible) {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
}

.picker__input {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

.picker__label {
  font-weight: 600;
}

.picker__description {
  color: var(--color-muted);
  font-size: 0.8125rem;
  line-height: 1.35;
}

.picker__option--unavailable .picker__label,
.picker__option--unavailable .picker__description {
  opacity: 0.65;
}

.picker__option--unavailable .picker__description {
  color: var(--color-warning);
}
</style>
