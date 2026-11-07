// Examples for the OpenAI-compatible API, in the same shape as the web
// console's (frontend/src/lib/apiSnippets.ts), so both teach the same call.
//
// With no key in hand they read it from an environment variable, so a copied
// snippet carries no secret. With a key created in this session the secret is
// embedded, so the snippet runs as pasted - shown masked, copied whole.

export type SnippetLanguage = 'shell' | 'python'

export const SNIPPET_LANGUAGES: { id: SnippetLanguage; label: string }[] = [
  { id: 'shell', label: 'Shell' },
  { id: 'python', label: 'Python' },
]

export const API_KEY_ENV_VAR = 'NODEA_API_KEY'

const PROMPT = 'Hello!'
const KEY_PLACEHOLDER = '<your API key>'

// Enough of the secret to tell which key it is, never enough to use it.
export function maskApiKey(value: string): string {
  return `${value.slice(0, 3)}***`
}

export function buildSnippet(
  language: SnippetLanguage,
  baseUrl: string,
  model: string,
  apiKey?: string | null
): string {
  switch (language) {
    case 'shell':
      return `export ${API_KEY_ENV_VAR}="${apiKey || KEY_PLACEHOLDER}"

curl ${baseUrl}/chat/completions \\
  -H "Authorization: Bearer $${API_KEY_ENV_VAR}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": "${model}",
    "messages": [{"role": "user", "content": "${PROMPT}"}]
  }'`

    case 'python':
      return `# pip install openai
${apiKey ? '' : 'import os\n\n'}from openai import OpenAI

client = OpenAI(
    base_url="${baseUrl}",
    api_key=${apiKey ? `"${apiKey}"` : `os.environ["${API_KEY_ENV_VAR}"]`},
)

completion = client.chat.completions.create(
    model="${model}",
    messages=[{"role": "user", "content": "${PROMPT}"}],
)

print(completion.choices[0].message.content)`
  }
}
