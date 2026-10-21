import { describe, expect, it } from 'vitest';

import {
  MAX_GLASS_LAYERS,
  detectRefractionSupport,
  resolveEffects,
  type EffectSettings,
  type EngineProbe,
} from './effects.ts';

const system = { reducedMotion: false, reducedTransparency: false };
const defaults: EffectSettings = {
  motion: 'system',
  transparency: 'system',
  refraction: 'auto',
  powerSaving: false,
};

describe('resolveEffects', () => {
  it('enables everything by default where refraction is supported', () => {
    expect(resolveEffects(system, defaults, true)).toEqual({
      motion: true,
      transparency: true,
      blur: true,
      refraction: true,
      animatedBackdrop: true,
      maxGlassLayers: MAX_GLASS_LAYERS,
    });
  });

  it('falls back to blur and highlight without refraction support', () => {
    const effects = resolveEffects(system, defaults, false);
    expect(effects.blur).toBe(true);
    expect(effects.refraction).toBe(false);
  });

  it('follows the system accessibility preferences', () => {
    const effects = resolveEffects(
      { reducedMotion: true, reducedTransparency: true },
      defaults,
      true,
    );
    expect(effects).toMatchObject({
      motion: false,
      animatedBackdrop: false,
      transparency: false,
      blur: false,
      refraction: false,
      maxGlassLayers: 0,
    });
  });

  it('lets the user override the system in both directions', () => {
    const forcedOn = resolveEffects(
      { reducedMotion: true, reducedTransparency: true },
      { ...defaults, motion: 'full', transparency: 'full' },
      false,
    );
    expect(forcedOn).toMatchObject({ motion: true, transparency: true, blur: true });

    const forcedOff = resolveEffects(
      system,
      { ...defaults, motion: 'reduce', transparency: 'reduce' },
      true,
    );
    expect(forcedOff).toMatchObject({ motion: false, transparency: false, blur: false });
  });

  it('power saving turns off blur, refraction and animation but keeps translucency', () => {
    expect(resolveEffects(system, { ...defaults, powerSaving: true }, true)).toEqual({
      motion: false,
      transparency: true,
      blur: false,
      refraction: false,
      animatedBackdrop: false,
      maxGlassLayers: 0,
    });
  });

  it('respects the refraction switch', () => {
    expect(resolveEffects(system, { ...defaults, refraction: 'off' }, true).refraction).toBe(false);
    expect(resolveEffects(system, { ...defaults, refraction: 'on' }, false).refraction).toBe(true);
  });
});

describe('detectRefractionSupport', () => {
  const probe = (parses: boolean, hasUserAgentData: boolean): EngineProbe => ({
    supports: (property) => parses && property === 'backdrop-filter',
    hasUserAgentData,
  });

  it('is supported by Blink (WebView2, Chromium)', () => {
    expect(detectRefractionSupport(probe(true, true))).toBe(true);
  });

  it('is not supported by WebKit, which parses but does not render it', () => {
    expect(detectRefractionSupport(probe(true, false))).toBe(false);
  });

  it('is not supported when backdrop-filter rejects url()', () => {
    expect(detectRefractionSupport(probe(false, true))).toBe(false);
  });

  it('accepts the prefixed property', () => {
    expect(
      detectRefractionSupport({
        supports: (property) => property === '-webkit-backdrop-filter',
        hasUserAgentData: true,
      }),
    ).toBe(true);
  });
});
