import { computed } from '@mickyballadelli/matrix'
import { Alert, Badge, Button, TrashIcon } from '@mickyballadelli/prism'

function formatDeviceDate(value) {
  if (!value) return 'Registration date unavailable'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return 'Unknown registration date'
  return new Intl.DateTimeFormat([], { dateStyle: 'medium', timeStyle: 'short' }).format(date)
}

export function AccountDevices({ devices, loading, error, revokingDeviceID, onRefresh, onRevoke }) {
  return computed(() => (
    <section class="account-devices" aria-labelledby="account-devices-title">
      <div class="account-devices-heading">
        <div>
          <h3 id="account-devices-title">Devices</h3>
          <p>Manage every device connected to this account.</p>
        </div>
        <Button label="Refresh" variant="tertiary" loading={loading} disabled={loading} onClick={onRefresh} />
      </div>
      {error.value ? <Alert tone="error">{error.value}</Alert> : null}
      {!loading.value && !error.value && devices.value.length === 0
        ? <p class="account-devices-empty">No registered devices found.</p>
        : null}
      <div class="account-devices-list">
        {devices.value.map(device => {
          const revoked = Boolean(device.revoked_at)
          const isRevoking = revokingDeviceID.value === device.device_id
          return (
            <div class={`account-device-row ${revoked ? 'is-revoked' : ''}`}>
              <div class="account-device-copy">
                <strong>{device.current ? 'This browser' : 'Linked device'}</strong>
                <small>{device.device_id}</small>
                <small>{formatDeviceDate(device.registered_at)} · {revoked ? 'Revoked' : device.delegation_role || 'Active'}</small>
              </div>
              {device.current
                ? <Badge value="Current" tone="success" size="small" />
                : revoked
                  ? <Badge value="Revoked" tone="neutral" size="small" />
                  : <Button
                      label="Revoke"
                      icon={<TrashIcon />}
                      variant="tertiary"
                      loading={isRevoking}
                      disabled={isRevoking || revokingDeviceID.value !== ''}
                      onClick={() => onRevoke(device)}
                    />}
            </div>
          )
        })}
      </div>
    </section>
  ))
}
