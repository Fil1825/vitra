import type { Preference, Switch } from '../lib/settings.ts';

/** Accessibility preferences reported by the OS through media queries. */
export interface SystemPreferences {
  reducedMotion: boolean;
  reducedTransparency: boolean;
}

export interface EffectSettings {
  motion: Preference;
  transparency: Preference;
  refraction: Switch;
  powerSaving: boolean;
}

/** Which visual effects are on right now. */
export interface Effects {
  /** Transitions and animations. */
  motion: boolean;
  /** Translucent surfaces; when off, glass and the window base become opaque. */
  transparency: boolean;
  /** `backdrop-filter` blur on glass surfaces. */
  blur: boolean;
  /** Edge refraction on glass surfaces. */
  refraction: boolean;
  /** The slowly flowing background gradient. */
  animatedBackdrop: boolean;
  /** How many glass surfaces may blur at once; the rest fall back to a solid tint. */
  maxGlassLayers: number;
}

/** Simultaneous `backdrop-filter` layers; each one costs a full-surface blur per frame. */
export const MAX_GLASS_LAYERS = 8;

function allowed(preference: Preference, systemReduced: boolean): boolean {
  if (preference === 'system') {
    return !systemReduced;
  }
  return preference === 'full';
}

/** Combines OS preferences, user settings and engine support. Power saving wins over everything. */
export function resolveEffects(
  system: SystemPreferences,
  settings: EffectSettings,
  supportsRefraction: boolean,
): Effects {
  const saving = settings.powerSaving;
  const motion = !saving && allowed(settings.motion, system.reducedMotion);
  const transparency = allowed(settings.transparency, system.reducedTransparency);
  const blur = transparency && !saving;
  const refractionWanted =
    settings.refraction === 'on' || (settings.refraction === 'auto' && supportsRefraction);

  return {
    motion,
    transparency,
    blur,
    refraction: blur && refractionWanted,
    animatedBackdrop: motion,
    maxGlassLayers: blur ? MAX_GLASS_LAYERS : 0,
  };
}

/** What we can ask the engine without rendering anything. */
export interface EngineProbe {
  supports(property: string, value: string): boolean;
  /** `navigator.userAgentData` (UA Client Hints) is present. */
  hasUserAgentData: boolean;
}

/**
 * Whether SVG filters work inside `backdrop-filter`, which refraction needs.
 *
 * `CSS.supports` alone is not enough: WebKit parses `backdrop-filter: url(#f)` but never renders
 * it. Only Blink renders it, and UA Client Hints (`navigator.userAgentData`) are a Blink-only
 * API, so together they identify the capability without looking at the OS or UA string.
 */
export function detectRefractionSupport(probe: EngineProbe): boolean {
  const parses =
    probe.supports('backdrop-filter', 'url(#vitra-probe) blur(1px)') ||
    probe.supports('-webkit-backdrop-filter', 'url(#vitra-probe) blur(1px)');
  return parses && probe.hasUserAgentData;
}

export function browserProbe(): EngineProbe {
  return {
    supports: (property, value) =>
      typeof CSS !== 'undefined' && typeof CSS.supports === 'function'
        ? CSS.supports(property, value)
        : false,
    hasUserAgentData: typeof navigator !== 'undefined' && 'userAgentData' in navigator,
  };
}
