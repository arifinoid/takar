export type HealthResponse = {
  status: string;
  service: string;
  version: string;
};

export type Role = "owner" | "admin" | "production" | "viewer";

export type TenantSummary = {
  id: string;
  name: string;
  role: Role;
};

export type AuthResponse = {
  access_token: string;
  token_type: string;
  expires_in: number;
  requires_tenant_selection: boolean;
  pending_invitation: boolean;
  tenants: TenantSummary[];
};

export type MeResponse = {
  user: {
    id: string;
    name: string;
    email: string | null;
    phone: string | null;
  };
  tenant: {
    id: string;
    name: string;
  };
  role: Role;
  permissions: string[];
};

export type OtpRequestResponse = {
  expires_in: number;
};

export type VerifyOtpRequest = {
  contact: string;
  code: string;
  name?: string;
  tenant_name?: string;
};

export type ApiClientOptions = {
  baseUrl: string;
  fetchImpl?: typeof fetch;
};

type ErrorBody = {
  error?: {
    code?: string;
    message?: string;
  };
};

/** A non-2xx response. Carries the API error code so the UI can map it. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

type RequestOptions = {
  method?: "GET" | "POST" | "PATCH" | "DELETE";
  body?: unknown;
  token?: string;
  signal?: AbortSignal;
};

export class ApiClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;

  constructor({ baseUrl, fetchImpl = fetch }: ApiClientOptions) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    // Native fetch throws "Illegal invocation" when called as a method of
    // another object, so keep it bound to the global scope.
    this.fetchImpl = fetchImpl.bind(globalThis);
  }

  private async request<T>(path: string, options: RequestOptions = {}): Promise<T> {
    const headers = new Headers();

    if (options.body !== undefined) {
      headers.set("content-type", "application/json");
    }

    if (options.token) {
      headers.set("authorization", `Bearer ${options.token}`);
    }

    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method: options.method ?? "GET",
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: options.signal,
    });

    if (response.status === 204) {
      return undefined as T;
    }

    const payload = (await response.json().catch(() => null)) as (T & ErrorBody) | null;

    if (!response.ok) {
      const error = payload?.error;
      throw new ApiError(
        response.status,
        error?.code ?? "request_failed",
        error?.message ?? `Request failed with status ${response.status}`,
      );
    }

    return payload as T;
  }

  health(signal?: AbortSignal): Promise<HealthResponse> {
    return this.request<HealthResponse>("/healthz", { signal });
  }

  requestOtp(contact: string): Promise<OtpRequestResponse> {
    return this.request<OtpRequestResponse>("/auth/otp/request", {
      method: "POST",
      body: { contact },
    });
  }

  verifyOtp(body: VerifyOtpRequest): Promise<AuthResponse> {
    return this.request<AuthResponse>("/auth/otp/verify", {
      method: "POST",
      body,
    });
  }

  listTenants(token: string): Promise<TenantSummary[]> {
    return this.request<TenantSummary[]>("/auth/tenants", { token });
  }

  selectTenant(token: string, tenantId: string): Promise<AuthResponse> {
    return this.request<AuthResponse>("/auth/select-tenant", {
      method: "POST",
      token,
      body: { tenant_id: tenantId },
    });
  }

  me(token: string): Promise<MeResponse> {
    return this.request<MeResponse>("/me", { token });
  }
}
