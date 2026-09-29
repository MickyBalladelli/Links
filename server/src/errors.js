// Error taxonomy mirrored from the Rust crates so HTTP status codes and
// gateway protocol error codes stay identical for every client.

export class ProtocolError extends Error {
  constructor(kind, field = '') {
    super(field ? `${kind}: ${field}` : kind)
    this.kind = kind // 'Invalid' | 'UnsupportedVersion' | 'TooLarge' | 'Malformed' | 'InvalidRetention'
  }
}

export class StoreError extends Error {
  constructor(kind, cause) {
    super(kind)
    this.kind = kind // 'Invalid' | 'Conflict' | 'NotFound' | 'Forbidden' | 'CursorExpired' | 'Unavailable' | 'CorruptObject' | 'Protocol' | 'Database' | 'Migration'
    this.cause = cause
  }
}

export class AuthError extends Error {
  constructor(kind) {
    super(kind)
    this.kind = kind // 'Invalid' | 'Denied' | 'NotFound' | 'RateLimited' | 'Conflict' | 'UsernameConflict' | 'DeviceConflict' | 'Unavailable'
  }
}

export class GatewayError extends Error {
  constructor(kind, cause) {
    super(kind)
    this.kind = kind // 'Invalid' | 'Authentication' | 'UnsupportedVersion' | 'Unavailable' | 'Conflict' | 'Protocol'
    this.cause = cause
  }
}

export class IdentityError extends Error {
  constructor(kind) {
    super(kind)
    this.kind = kind // 'Invalid' | 'Authentication'
  }
}

export const authError = kind => new AuthError(kind)

export function storeErrorFromDatabase(error) {
  if (error instanceof StoreError) {
    return error
  }
  if (error instanceof ProtocolError) {
    return new StoreError('Protocol', error)
  }
  switch (error?.code) {
    case '23505':
      return new StoreError('Conflict', error)
    case '23503':
    case '23514':
    case '23502':
    case '22P02':
      return new StoreError('Invalid', error)
    default:
      return new StoreError('Database', error)
  }
}

export function authErrorFrom(error) {
  if (error instanceof AuthError) {
    return error
  }
  if (error instanceof IdentityError) {
    return new AuthError('Denied')
  }
  if (error instanceof StoreError) {
    switch (error.kind) {
      case 'Invalid':
      case 'Protocol':
        return new AuthError('Invalid')
      case 'Conflict':
        return new AuthError('Conflict')
      case 'Forbidden':
      case 'NotFound':
        return new AuthError('Denied')
      default:
        return new AuthError('Unavailable')
    }
  }
  // Database driver, network, and unexpected runtime failures.
  return new AuthError('Unavailable')
}

export function gatewayErrorFrom(error) {
  if (error instanceof GatewayError) {
    return error
  }
  if (error instanceof ProtocolError) {
    return new GatewayError('Protocol', error)
  }
  if (error instanceof StoreError) {
    if (error.kind === 'Invalid') {
      return new GatewayError('Invalid', error)
    }
    if (error.kind === 'Protocol') {
      if (error.cause?.kind === 'UnsupportedVersion') {
        return new GatewayError('UnsupportedVersion', error)
      }
      return new GatewayError('Invalid', error)
    }
    if (error.kind === 'Forbidden' || error.kind === 'NotFound') {
      return new GatewayError('Authentication', error)
    }
    if (error.kind === 'Conflict') {
      return new GatewayError('Conflict', error)
    }
    return new GatewayError('Unavailable', error)
  }
  return new GatewayError('Unavailable', error)
}
