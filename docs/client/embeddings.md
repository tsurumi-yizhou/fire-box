# Embeddings

This document specifies the embedding interface of the FireBox Client SDK, enabling callers to transform text into high-dimensional vector representations. All operations require a connected `FireBoxClient` instance (see `@client/connection.md`).

## Capability Requirement

Embedding is an optional capability. Before calling the embedding API, callers SHOULD verify that the target model supports embeddings:

```
model = client.getModelMetadata(model_id = "embedding-model")
if not model.capabilities.embeddings:
    // This model does not support embeddings
```

Sending an embed request to a model without `embeddings = true` results in an `UnsupportedCapability` error.

## Embed

Transforms one or more text inputs into vector representations.

### API

```
response = client.embed(
    model_id        = "embedding-model",
    inputs          = ["Hello world", "Goodbye world"],
    encoding_format = "float"    // Optional: "float" (default) or "base64"
)
```

### Parameters

| Parameter | Type | Required | Description |
|---|---|---|---|
| `model_id` | string | Yes | The virtual model ID with embedding capability |
| `inputs` | list of string | Yes | One or more text inputs to embed |
| `encoding_format` | string | No | Output format: `"float"` (default) or `"base64"` |

### Response

| Field | Type | Description |
|---|---|---|
| `embeddings` | list of Embedding | One embedding per input, in the same order |
| `usage` | Usage | Token consumption statistics |

Each `Embedding` contains:

| Field | Type | Description |
|---|---|---|
| `values` | list of double | The embedding vector (when `encoding_format = "float"`) |
| `index` | int | The index of the corresponding input |

### Example

```
response = client.embed(
    model_id = "embedding-model",
    inputs   = [
        "The quick brown fox jumps over the lazy dog",
        "A fast auburn canine leaps above an idle hound"
    ]
)

vec_a = response.embeddings[0].values  // e.g., [0.012, -0.034, ...]
vec_b = response.embeddings[1].values

similarity = cosine_similarity(vec_a, vec_b)
print(similarity)  // e.g., 0.94
```

### Batch Processing

The `inputs` parameter accepts multiple strings, enabling efficient batch embedding without issuing separate requests. All inputs are embedded in a single round-trip.

```
documents = load_documents()  // e.g., 100 text chunks
response = client.embed(
    model_id = "embedding-model",
    inputs   = documents
)

for emb in response.embeddings:
    store_vector(index = emb.index, vector = emb.values)
```

Implementors SHOULD be aware that the backend may impose limits on the number of inputs per request or the total input token count. If a batch exceeds these limits, the backend returns an `InvalidRequest` error. The SDK does not perform automatic batching or chunking — callers are responsible for splitting large batches if necessary.

## Protocol Mapping Reference

| SDK Method | Request Message | Response Message |
|---|---|---|
| `client.embed(...)` | `EmbedRequest` | `EmbedResponse` |
