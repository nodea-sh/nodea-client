import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { NodePhase, NodeState } from '@/hooks/useNodePhase'
import { NodeStatusWidget } from './NodeStatusWidget'

function node(phase: NodePhase, overrides: Partial<NodeState> = {}): NodeState {
  return {
    phase,
    modelName: 'Qwen2.5-Coder-7B',
    downloadProgress: 62,
    downloadedBytes: 0,
    totalBytes: 0,
    bytesPerSec: 0,
    sharing: phase !== 'paused',
    setSharing: vi.fn(),
    retry: vi.fn(),
    orgHasModels: true,
    orgModelRef: null,
    ...overrides,
  }
}

describe('NodeStatusWidget (#317)', () => {
  afterEach(cleanup)

  it('says "Sharing" with no percentage when sharing is on, even with the node idle', () => {
    const { container } = render(<NodeStatusWidget node={node('serving')} />)

    expect(screen.getAllByText('Sharing').length).toBeGreaterThan(0)
    expect(screen.queryByTestId('widget-value')).toBeNull()
    expect(container.textContent).not.toContain('100%')
  })

  it('shows no percentage while paused either', () => {
    const { container } = render(<NodeStatusWidget node={node('paused')} />)

    expect(screen.queryByTestId('widget-value')).toBeNull()
    expect(container.textContent).not.toContain('%')
  })

  it('still shows real download progress', () => {
    render(<NodeStatusWidget node={node('downloading')} />)

    expect(screen.getByTestId('widget-value').textContent).toBe('62%')
  })
})
