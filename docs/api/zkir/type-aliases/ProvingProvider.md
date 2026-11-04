[**@midnight/zkir**](../README.md)

***

[@midnight/zkir](../README.md) / ProvingProvider

# Type Alias: ProvingProvider

> **ProvingProvider** = `object` & [`KeyMaterialProvider`](KeyMaterialProvider.md)

Defined in: [zkir.d.ts:2](https://github.com/midnightntwrk/midnight-zkir/blob/7c33a67ed770802969c7b2048a84cc6f9256308a/zkir-wasm/zkir.d.ts#L2)

## Type Declaration

### check()

> **check**(`serializedPreimage`, `keyLocation`): `Promise`\<(`bigint` \| `undefined`)[]\>

#### Parameters

##### serializedPreimage

`Uint8Array`

##### keyLocation

`string`

#### Returns

`Promise`\<(`bigint` \| `undefined`)[]\>

### prove()

> **prove**(`serializedPreimage`, `keyLocation`, `overwriteBindingInput?`): `Promise`\<`Uint8Array`\<`ArrayBufferLike`\>\>

#### Parameters

##### serializedPreimage

`Uint8Array`

##### keyLocation

`string`

##### overwriteBindingInput?

`bigint`

#### Returns

`Promise`\<`Uint8Array`\<`ArrayBufferLike`\>\>
