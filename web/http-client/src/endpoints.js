export const endpointGroups = [
  {
    name: 'Session',
    endpoints: [
      { name: 'Current account', method: 'GET', path: '/v1/auth/me', auth: 'bearer' },
      { name: 'Log out', method: 'POST', path: '/v1/auth/logout', auth: 'bearer' },
      { name: 'Revoke other sessions', method: 'DELETE', path: '/v1/auth/sessions/others', auth: 'bearer' }
    ]
  },
  {
    name: 'Username auth',
    endpoints: [
      {
        name: 'Create challenge',
        method: 'POST',
        path: '/v1/auth/username/challenge',
        auth: 'none',
        body: {
          handle: 'alice',
          purpose: 'login',
          device_id: '<device UUID>',
          mls_node_id: '<MLS node UUID>',
          public_key: '<base64url public key>'
        }
      },
      {
        name: 'Register',
        method: 'POST',
        path: '/v1/auth/username/register',
        auth: 'none',
        body: {
          challenge_id: '<challenge UUID>',
          signature: '<base64url signature>'
        }
      },
      {
        name: 'Log in',
        method: 'POST',
        path: '/v1/auth/username/login',
        auth: 'none',
        body: {
          challenge_id: '<challenge UUID>',
          signature: '<base64url signature>'
        }
      }
    ]
  },
  {
    name: 'Account profile',
    endpoints: [
      { name: 'Read display name', method: 'GET', path: '/v1/account/display-name', auth: 'bearer' },
      {
        name: 'Change display name',
        method: 'PUT',
        path: '/v1/account/display-name',
        auth: 'bearer',
        body: { display_name: 'Ava' }
      }
    ]
  },
  {
    name: 'Directory',
    endpoints: [
      { name: 'Find by username', method: 'GET', path: '/v1/directory/alice', auth: 'none' },
      {
        name: 'Find by user ID',
        method: 'GET',
        path: '/v1/directory/users/<user UUID>',
        auth: 'bearer'
      },
      {
        name: 'Sync saved contact profiles',
        method: 'POST',
        path: '/v1/directory/profiles/sync',
        auth: 'bearer',
        body: { user_ids: ['<user UUID>'] }
      }
    ]
  },
  {
    name: 'Devices and keys',
    endpoints: [
      { name: 'Register device', method: 'POST', path: '/v1/devices', auth: 'bearer', body: {} },
      { name: 'Pre-key inventory', method: 'GET', path: '/v1/prekeys/status', auth: 'bearer' },
      { name: 'Upload pre-keys', method: 'PUT', path: '/v1/prekeys', auth: 'bearer', body: {} },
      {
        name: 'Download MLS key package',
        method: 'GET',
        path: '/v1/mls/key-package/<device UUID>',
        auth: 'bearer'
      }
    ]
  },
  {
    name: 'Groups',
    endpoints: [
      { name: 'Create group', method: 'POST', path: '/v1/groups', auth: 'bearer', body: {} },
      {
        name: 'List members',
        method: 'GET',
        path: '/v1/groups/<group UUID>/members',
        auth: 'bearer'
      }
    ]
  },
  {
    name: 'Admin',
    endpoints: [
      { name: 'List users', method: 'GET', path: '/v1/admin/users', auth: 'admin' },
      {
        name: 'Set user status',
        method: 'PUT',
        path: '/v1/admin/users/<user UUID>/status',
        auth: 'admin',
        body: { disabled: true }
      },
      {
        name: 'Delete user',
        method: 'DELETE',
        path: '/v1/admin/users/<user UUID>',
        auth: 'admin'
      }
    ]
  }
]
