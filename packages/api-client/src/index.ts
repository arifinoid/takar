export type HealthResponse = {
  status: string;
  service: string;
  version: string;
};

export type ApiClientOptions = {
  baseUrl: string;
  fetchImpl?: typeof fetch;
};

export class ApiClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;

  constructor({ baseUrl, fetchImpl = fetch }: ApiClientOptions) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.fetchImpl = fetchImpl;
  }

  async health(signal?: AbortSignal): Promise<HealthResponse> {
    const response = await this.fetchImpl(`${this.baseUrl}/healthz`, { signal });

    if (!response.ok) {
      throw new Error(`Health check failed with status ${response.status}`);
    }

    return (await response.json()) as HealthResponse;
  }
}
