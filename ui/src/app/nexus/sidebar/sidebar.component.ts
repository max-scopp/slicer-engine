import {
  afterNextRender,
  Component,
  computed,
  DOCUMENT,
  ElementRef,
  HostListener,
  inject,
  OnDestroy,
  Renderer2,
  signal,
} from '@angular/core';
import { RouterLink } from '@angular/router';
import { ConnectionState } from '../../components/connection-state/connection-state';
import { Logo } from '../../components/logo/logo';
import { AppTheme } from '../../services/app-theme';
import { Icon } from '../../shared/icon/icon';

const STORAGE_WIDTH_KEY = 'nexus.sidebar.width';
const STORAGE_COLLAPSED_KEY = 'nexus.sidebar.collapsed';
const DEFAULT_WIDTH = 280;
const MIN_WIDTH = 180;
const MAX_WIDTH = 480;

@Component({
  selector: 'nexus-sidebar',
  standalone: true,
  imports: [Logo, RouterLink, ConnectionState, Icon],
  templateUrl: './sidebar.component.html',
  styleUrl: './sidebar.component.scss',
  host: {
    '(mouseenter)': 'onMouseEnter()',
    '(mouseleave)': 'onMouseLeave()',
    '(click)': 'onPanelClick()',
    '[class.is-collapsed]': 'collapsed()',
    '[class.is-expanded]': 'isExpanded()',
    '[class.is-dragging]': 'isDragging()',
  },
})
export class Sidebar implements OnDestroy {
  protected readonly _theme = inject(AppTheme);

  private readonly el = inject(ElementRef<HTMLElement>);
  private readonly renderer = inject(Renderer2);
  private readonly document = inject(DOCUMENT);

  protected readonly collapsed = signal(this.readCollapsed());
  protected readonly pinnedOpen = signal(false);
  protected readonly hovered = signal(false);
  protected readonly isDragging = signal(false);

  protected readonly isExpanded = computed(
    () => !this.collapsed() || this.pinnedOpen() || this.hovered(),
  );

  private dragStartX = 0;
  private dragStartWidth = 0;
  private dragCleanup: (() => void)[] = [];

  constructor() {
    afterNextRender(() => {
      this.applyCssWidth(this.readWidth());
    });
  }

  ngOnDestroy(): void {
    for (const fn of this.dragCleanup) {
      fn();
    }
  }

  protected onCollapseToggle(event: MouseEvent): void {
    event.stopPropagation();
    const next = !this.collapsed();
    this.collapsed.set(next);
    if (next) {
      this.pinnedOpen.set(false);
      this.hovered.set(false);
    }
    this.saveCollapsed(next);
  }

  protected onMouseEnter(): void {
    if (this.collapsed()) {
      this.hovered.set(true);
    }
  }

  protected onMouseLeave(): void {
    this.hovered.set(false);
  }

  protected onPanelClick(): void {
    if (this.collapsed() && !this.pinnedOpen()) {
      this.pinnedOpen.set(true);
    }
  }

  protected onPinToggle(event: MouseEvent): void {
    event.stopPropagation();
    this.pinnedOpen.update((v) => !v);
  }

  @HostListener('document:click', ['$event'])
  protected onDocumentClick(event: MouseEvent): void {
    if (this.collapsed() && this.pinnedOpen()) {
      if (!this.el.nativeElement.contains(event.target as Node)) {
        this.pinnedOpen.set(false);
      }
    }
  }

  @HostListener('document:keydown.escape')
  protected onEscape(): void {
    if (this.collapsed() && this.pinnedOpen()) {
      this.pinnedOpen.set(false);
    }
  }

  protected onResizeStart(event: MouseEvent): void {
    if (this.collapsed()) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    this.isDragging.set(true);
    this.dragStartX = event.clientX;
    this.dragStartWidth = this.el.nativeElement.offsetWidth;

    let rafId: number | null = null;
    let latestX = this.dragStartX;

    const onMove = (e: MouseEvent): void => {
      latestX = e.clientX;
      if (rafId !== null) {
        return;
      }
      rafId = requestAnimationFrame(() => {
        rafId = null;
        const delta = latestX - this.dragStartX;
        const width = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, this.dragStartWidth + delta));
        this.applyCssWidth(width);
      });
    };

    const onUp = (): void => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
      this.document.removeEventListener('mousemove', onMove);
      this.document.removeEventListener('mouseup', onUp);
      this.isDragging.set(false);
      this.saveWidth(this.el.nativeElement.offsetWidth);
    };

    this.document.addEventListener('mousemove', onMove);
    this.document.addEventListener('mouseup', onUp);
    this.dragCleanup = [
      () => this.document.removeEventListener('mousemove', onMove),
      () => this.document.removeEventListener('mouseup', onUp),
    ];
  }

  private applyCssWidth(width: number): void {
    this.el.nativeElement.style.setProperty('--sidebar-w', `${width}px`);
  }

  private readWidth(): number {
    try {
      const stored = localStorage.getItem(STORAGE_WIDTH_KEY);
      if (stored) {
        const parsed = parseInt(stored, 10);
        if (!isNaN(parsed)) {
          return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, parsed));
        }
      }
    } catch {
      // storage unavailable
    }
    return DEFAULT_WIDTH;
  }

  private saveWidth(width: number): void {
    try {
      localStorage.setItem(STORAGE_WIDTH_KEY, String(width));
    } catch {
      // storage unavailable
    }
  }

  private readCollapsed(): boolean {
    try {
      return localStorage.getItem(STORAGE_COLLAPSED_KEY) === 'true';
    } catch {
      return false;
    }
  }

  private saveCollapsed(value: boolean): void {
    try {
      localStorage.setItem(STORAGE_COLLAPSED_KEY, String(value));
    } catch {
      // storage unavailable
    }
  }
}
