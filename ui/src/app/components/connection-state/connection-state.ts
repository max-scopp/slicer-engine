import { Component, computed, inject } from '@angular/core';
import { SlicerConnection } from '../../services/slicer-connection';
import { Badge, BadgeVariant } from '../../shared/badge/badge';

interface StatusConfig {
  label: string;
  variant: BadgeVariant;
  pulse: boolean;
}

const STATUS_CONFIG: Record<string, StatusConfig> = {
  connecting: { label: 'Connecting…', variant: 'warning', pulse: true },
  connected: { label: 'Connected', variant: 'success', pulse: false },
  disconnected: { label: 'Disconnected', variant: 'default', pulse: false },
  failed: { label: '', variant: 'danger', pulse: false },
};

@Component({
  selector: 'nexus-connection-state',
  standalone: true,
  imports: [Badge],
  templateUrl: './connection-state.html',
  styleUrl: './connection-state.scss',
})
export class ConnectionState {
  readonly connection = inject(SlicerConnection);

  readonly config = computed<StatusConfig>(
    () => STATUS_CONFIG[this.connection.status()] ?? STATUS_CONFIG['disconnected'],
  );
}
