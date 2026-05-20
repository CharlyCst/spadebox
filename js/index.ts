export type { SbTool, SbToolResult } from './napi_generated.js'

import { SpadeBox as _SpadeBox } from './napi_generated.js'

export class SpadeBox extends _SpadeBox {
  /** @inheritDoc */
  override exposeJsFunc(
    name: string,
    params: string[],
    func: (args: Record<string, unknown>) => unknown | Promise<unknown>,
  ): this {
    return super.exposeJsFunc(name, params, async (args) => await func(args))
  }
}
