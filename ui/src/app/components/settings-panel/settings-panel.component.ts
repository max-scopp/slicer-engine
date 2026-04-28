import { Component, inject } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MarkdownComponent } from 'ngx-markdown';
import { SlicerService } from '../../services/slicer.service';
import { WsSlicingParams } from '../../../generated/slicer-engine-ws-client-message-v1';
import wsClientSchema from '../../../schemas/slicer-engine-ws-client-message-v1.json';

interface FieldDef {
    key: keyof WsSlicingParams;
    type: 'number' | 'string';
    description: string;
    default?: number | string;
    required: boolean;
    enumValues?: string[];
}

function resolveEnumValues(prop: Record<string, unknown>): string[] | undefined {
    // Field references a $def (e.g. { "$ref": "#/$defs/GcodeFlavor" })
    if ('$ref' in prop) {
        const ref = prop['$ref'] as string;
        const defName = ref.replace('#/$defs/', '');
        const defs = wsClientSchema.$defs as Record<string, { oneOf?: Array<{ const?: unknown }> }>;
        const def = defs[defName];
        if (def?.oneOf) {
            return def.oneOf.map(v => String(v.const));
        }
    }
    // Field has a direct oneOf (less common)
    if ('oneOf' in prop) {
        const oneOf = prop['oneOf'] as Array<{ const?: unknown }>;
        return oneOf.map(v => String(v.const));
    }
    return undefined;
}

function buildFields(): FieldDef[] {
    const paramsSchema = wsClientSchema.$defs.WsSlicingParams;
    const required = new Set<string>(paramsSchema.required);
    return Object.entries(paramsSchema.properties).map(([key, prop]) => {
        const p = prop as Record<string, unknown>;
        const isNumber = p['type'] === 'number';
        return {
            key: key as keyof WsSlicingParams,
            type: isNumber ? 'number' : 'string',
            description: (p['description'] as string | undefined) ?? key,
            default: p['default'] as number | string | undefined,
            required: required.has(key),
            enumValues: resolveEnumValues(p),
        };
    });
}

@Component({
    selector: 'nexus-settings-panel',
    standalone: true,
    imports: [FormsModule, MarkdownComponent],
    templateUrl: './settings-panel.component.html',
    styleUrl: './settings-panel.component.scss',
})
export class SettingsPanelComponent {
    private readonly slicer = inject(SlicerService);

    readonly settings = this.slicer.settings;
    readonly fields: FieldDef[] = buildFields();

    update(key: keyof WsSlicingParams, rawValue: string): void {
        const field = this.fields.find(f => f.key === key);
        const value: string | number = field?.type === 'number' ? +rawValue : rawValue;
        this.slicer.updateSettings({ [key]: value } as Partial<WsSlicingParams>);
    }
}
