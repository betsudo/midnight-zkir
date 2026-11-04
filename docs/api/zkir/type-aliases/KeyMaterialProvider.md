[**@midnight/zkir**](../README.md)

***

[@midnight/zkir](../README.md) / KeyMaterialProvider

# Type Alias: KeyMaterialProvider

> **KeyMaterialProvider** = `object`

Defined in: [zkir.d.ts:20](https://github.com/midnightntwrk/midnight-zkir/blob/7c33a67ed770802969c7b2048a84cc6f9256308a/zkir-wasm/zkir.d.ts#L20)

## Methods

### getParams()

> **getParams**(`k`): `Promise`\<`Uint8Array`\<`ArrayBufferLike`\>\>

Defined in: [zkir.d.ts:22](https://github.com/midnightntwrk/midnight-zkir/blob/7c33a67ed770802969c7b2048a84cc6f9256308a/zkir-wasm/zkir.d.ts#L22)

#### Parameters

##### k

`number`

#### Returns

`Promise`\<`Uint8Array`\<`ArrayBufferLike`\>\>

***

### lookupKey()

> **lookupKey**(`keyLocation`): `Promise`\<[`ProvingKeyMaterial`](ProvingKeyMaterial.md) \| `undefined`\>

Defined in: [zkir.d.ts:21](https://github.com/midnightntwrk/midnight-zkir/blob/7c33a67ed770802969c7b2048a84cc6f9256308a/zkir-wasm/zkir.d.ts#L21)

#### Parameters

##### keyLocation

`string`

#### Returns

`Promise`\<[`ProvingKeyMaterial`](ProvingKeyMaterial.md) \| `undefined`\>
