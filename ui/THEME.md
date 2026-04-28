# Theme System

The UI uses a CSS variable-based theming system supporting light and dark modes.

## CSS Variables

All theme-related styles use CSS custom properties defined in `src/styles.scss`:

### Colors

- `--color-bg-primary`: Main background
- `--color-bg-secondary`: Secondary background
- `--color-bg-tertiary`: Tertiary background
- `--color-surface`: Interactive surface (buttons, inputs)
- `--color-text-primary`: Primary text
- `--color-text-secondary`: Secondary text
- `--color-text-tertiary`: Tertiary/disabled text
- `--color-border`: Border color
- `--color-accent`: Primary accent color
- `--color-success`, `--color-warning`, `--color-error`: Status colors

### Spacing

- `--spacing-xs` (4px) to `--spacing-2xl` (32px)

### Radius

- `--radius-sm` (4px), `--radius-md` (6px), `--radius-lg` (8px)

### Transitions

- `--transition-fast` (0.15s), `--transition-normal` (0.2s), `--transition-slow` (0.3s)

## Mode Selection

Modes are controlled via the `html` element's class:

- **Light Mode** (default): `<html>`
- **Dark Mode**: `<html class="dark">`

## Using the Theme Service

Inject and use the `ThemeService` to toggle themes:

```typescript
import { ThemeService } from './services/theme.service';

constructor(private themeService: ThemeService) {}

toggleDarkMode() {
  this.themeService.toggleTheme();
}

getCurrentTheme() {
  return this.themeService.currentTheme(); // true = dark, false = light
}
```

## In Components

Use CSS variables directly in your component styles:

```scss
.my-component {
  background-color: var(--color-bg-secondary);
  color: var(--color-text-primary);
  padding: var(--spacing-md);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  transition: all var(--transition-normal);
}
```

## Persisting Theme Preference

The theme preference is automatically saved to `localStorage` and restored on page load. System preference (`prefers-color-scheme`) is used as fallback.
