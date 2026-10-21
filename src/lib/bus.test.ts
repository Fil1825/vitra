import { afterEach, describe, expect, it, vi } from 'vitest';

import { createBus } from './bus.ts';

interface TestEvents {
  ping: number;
  other: string;
}

describe('createBus', () => {
  afterEach(() => vi.restoreAllMocks());

  it('delivers payloads to subscribers of that event only', () => {
    const bus = createBus<TestEvents>();
    const ping = vi.fn();
    const other = vi.fn();
    bus.on('ping', ping);
    bus.on('other', other);

    bus.emit('ping', 42);

    expect(ping).toHaveBeenCalledWith(42);
    expect(other).not.toHaveBeenCalled();
  });

  it('stops delivering after unsubscribe', () => {
    const bus = createBus<TestEvents>();
    const handler = vi.fn();
    const off = bus.on('ping', handler);
    off();
    bus.emit('ping', 1);
    expect(handler).not.toHaveBeenCalled();
  });

  it('once delivers a single event', () => {
    const bus = createBus<TestEvents>();
    const handler = vi.fn();
    bus.once('ping', handler);
    bus.emit('ping', 1);
    bus.emit('ping', 2);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith(1);
  });

  it('keeps delivering when one handler throws', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const bus = createBus<TestEvents>();
    const after = vi.fn();
    bus.on('ping', () => {
      throw new Error('boom');
    });
    bus.on('ping', after);

    bus.emit('ping', 7);

    expect(after).toHaveBeenCalledWith(7);
    expect(console.error).toHaveBeenCalled();
  });

  it('lets handlers unsubscribe while an event is delivered', () => {
    const bus = createBus<TestEvents>();
    const second = vi.fn();
    const off = bus.on('ping', () => off());
    bus.on('ping', second);
    bus.emit('ping', 1);
    bus.emit('ping', 2);
    expect(second).toHaveBeenCalledTimes(2);
  });
});
