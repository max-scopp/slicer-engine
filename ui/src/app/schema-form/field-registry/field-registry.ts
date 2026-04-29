import { Type } from '@angular/core';
import { InfillDensitySliderComponent } from '../custom-widgets/infill-density-slider/infill-density-slider.component';
import { InfillPatternPickerComponent } from '../custom-widgets/infill-pattern-picker/infill-pattern-picker.component';
import { FieldDef } from '../models/field-def';
import { FieldWidget } from '../widgets/base-field';
import { BooleanFieldComponent } from '../widgets/boolean-field/boolean-field.component';
import { EnumRadioComponent } from '../widgets/enum-radio/enum-radio.component';
import { EnumSelectComponent } from '../widgets/enum-select/enum-select.component';
import { IntegerFieldComponent } from '../widgets/integer-field/integer-field.component';
import { NumberFieldComponent } from '../widgets/number-field/number-field.component';

/**
 * Maximum number of enum options for which a radio group is used.
 * Fields with more options than this threshold render as a `<select>` dropdown.
 */
const RADIO_MAX_OPTIONS = 3;

/**
 * Key-specific widget overrides.
 * Add an entry here to swap in a custom widget for any schema field key.
 */
const KEY_REGISTRY: Record<string, Type<FieldWidget>> = {
  infill_density: InfillDensitySliderComponent,
  infill_pattern: InfillPatternPickerComponent,
};

/**
 * Select the default widget for a field based on its type and enum cardinality.
 */
function defaultWidgetFor(field: FieldDef): Type<FieldWidget> {
  if (field.enumOptions) {
    return field.enumOptions.length <= RADIO_MAX_OPTIONS ? EnumRadioComponent : EnumSelectComponent;
  }

  switch (field.type) {
    case 'integer':
      return IntegerFieldComponent;
    case 'boolean':
      return BooleanFieldComponent;
    default:
      return NumberFieldComponent;
  }
}

/**
 * Resolve the widget component class for a given field.
 * Key-specific overrides take precedence over the type-based default.
 */
export function resolveWidget(field: FieldDef): Type<FieldWidget> {
  return KEY_REGISTRY[field.key] ?? defaultWidgetFor(field);
}
