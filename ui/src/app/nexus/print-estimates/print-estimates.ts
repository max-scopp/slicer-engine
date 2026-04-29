import { Component, input } from '@angular/core';
import { Card } from '../../components/card/card';

export interface PrintEstimatesDto {
  status: 'ready' | 'modified';
  time: string;
  material: string;
  materialDelta?: string;
  cost: string;
  layers: number;
}

@Component({
  selector: 'nexus-print-estimates',
  imports: [Card],
  templateUrl: './print-estimates.html',
  styleUrl: './print-estimates.scss',
})
export class PrintEstimates {
  estimates = input<PrintEstimatesDto>({
    status: 'ready',
    time: '2h 45m',
    material: '32g',
    materialDelta: '-10.8m',
    cost: '$0.85',
    layers: 245,
  });
}
