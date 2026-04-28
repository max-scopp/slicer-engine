import { Injectable, signal } from '@angular/core';

@Injectable({
  providedIn: 'root',
})
export class AppTheme {
  private readonly isDarkMode = signal(this.getInitialTheme());

  readonly currentTheme = this.isDarkMode.asReadonly();

  constructor() {
    this.applyTheme(this.isDarkMode());
  }

  toggleTheme(): void {
    const newMode = !this.isDarkMode();
    this.isDarkMode.set(newMode);
    this.applyTheme(newMode);
    localStorage.setItem('theme', newMode ? 'dark' : 'light');
  }

  setTheme(isDark: boolean): void {
    this.isDarkMode.set(isDark);
    this.applyTheme(isDark);
    localStorage.setItem('theme', isDark ? 'dark' : 'light');
  }

  private applyTheme(isDark: boolean): void {
    const htmlElement = document.documentElement;
    if (isDark) {
      htmlElement.classList.add('dark');
    } else {
      htmlElement.classList.remove('dark');
    }
  }

  private getInitialTheme(): boolean {
    // Check localStorage first
    const stored = localStorage.getItem('theme');
    if (stored) {
      return stored === 'dark';
    }

    // Check system preference
    if (window.matchMedia) {
      return window.matchMedia('(prefers-color-scheme: dark)').matches;
    }

    // Default to light mode
    return false;
  }
}
