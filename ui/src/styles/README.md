# Styles Directory

Modular SCSS architecture using `@use` for better organization and maintainability.

## Structure

```
styles/
├── main.scss               # Entry point - imports all modules
├── _mixins.scss            # Reusable SCSS mixins and functions
├── _variables.scss         # DEPRECATED - use theme/ modules instead
│
├── base/                   # Core element styles
│   ├── _reset.scss         # Box-sizing reset
│   ├── _typography.scss    # Headings, paragraphs, links
│   ├── _forms.scss         # Button, input, select, textarea
│   └── _scrollbar.scss     # Custom scrollbar styles
│
├── theme/                  # Design tokens and mode switching
│   ├── _tokens.scss        # Shared tokens (spacing, radius, transitions)
│   ├── _light.scss         # Light mode CSS variables
│   └── _dark.scss          # Dark mode CSS variables (applied via html.dark)
│
└── utilities/              # Utility classes
    └── _utilities.scss     # Text, background, border, spacing utilities
```

## Usage

### In Components

Import CSS variables and use them in component styles:

```scss
// component.component.scss
.my-element {
  background-color: var(--color-bg-secondary);
  color: var(--color-text-primary);
  padding: var(--spacing-md);
  border-radius: var(--radius-md);
  transition: all var(--transition-normal);
}
```

### Using Tokens in SCSS

If you need tokens in component SCSS, use `@use`:

```scss
@use 'src/styles/theme/tokens' as tokens;

.my-element {
  padding: tokens.$spacing-lg;
  border-radius: tokens.$radius-md;
}
```

### Using Mixins

Reusable mixins are available via `@use`:

```scss
@use 'src/styles/mixins' as *;

.text-overflow {
  @include text-ellipsis(2);
}

.button {
  @include flex-center;
  @include transition(background-color);

  &:focus-visible {
    @include focus-outline;
  }
}
```

## Theme System

### Switching Modes

Use the `ThemeService` to toggle between light and dark modes:

```typescript
constructor(private themeService: ThemeService) {}

toggleDarkMode() {
  this.themeService.toggleTheme();
}
```

**How it works:**

- Light mode (default): `<html>` with no class
- Dark mode: `<html class="dark">`
- All CSS variables automatically update via CSS cascading

### CSS Variables Available

**Colors:**

- `--color-bg-primary`, `--color-bg-secondary`, `--color-bg-tertiary`
- `--color-surface`, `--color-surface-hover`, `--color-surface-active`
- `--color-text-primary`, `--color-text-secondary`, `--color-text-tertiary`
- `--color-border`, `--color-border-light`
- `--color-accent`, `--color-accent-hover`, `--color-accent-dark`
- `--color-success`, `--color-success-bg`
- `--color-warning`, `--color-warning-bg`
- `--color-error`, `--color-error-bg`

**Spacing:**

- `--spacing-xs` (4px) through `--spacing-2xl` (32px)

**Radius:**

- `--radius-sm`, `--radius-md`, `--radius-lg`

**Transitions:**

- `--transition-fast` (0.15s), `--transition-normal` (0.2s), `--transition-slow` (0.3s)

**Scrollbar:**

- `--color-scrollbar-track`, `--color-scrollbar-thumb`, `--color-scrollbar-thumb-hover`

## Adding New Styles

1. **New base element styles** → Create in `base/` folder
2. **New mixins** → Add to `_mixins.scss`
3. **New tokens** → Add to `theme/_tokens.scss` (both light and dark)
4. **New utility classes** → Add to `utilities/_utilities.scss`

Then import in `main.scss`:

```scss
@use './base/my-new-styles';
@use './utilities/my-new-utilities';
```

## Sass @use vs @import

This project uses the modern `@use` system (Dart Sass) instead of deprecated `@import`:

- **`@use`**: Creates a module namespace, better scoping, no duplication
- **`@import`**: Deprecated, causes style duplication, global scope

All modules should use `@use` and leverage namespacing:

```scss
@use './theme/tokens' as tokens;

$my-spacing: tokens.$spacing-md;
```

## Performance Notes

- **Tree-shaking friendly**: Each module is separate and easily identifiable
- **No style duplication**: Variables defined once, referenced everywhere
- **Minimal CSS output**: ~1.15 kB gzipped for full theme system
- **CSS Variables advantage**: Theme switching requires no JavaScript file parsing
