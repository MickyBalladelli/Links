use super::*;

const HANDLE: &[u8; 36] = b"01234567-89ab-4cde-8fab-0123456789ab";
#[derive(Default)]
struct Fixture {
    seed: Option<Zeroizing<[u8; 32]>>,
    stores: usize,
    loads: usize,
    deletes: usize,
    store_status: i32,
    load_status: i32,
    delete_status: i32,
}
unsafe extern "C" fn save(ctx: *mut c_void, seed: *const u8, output: *mut u8) -> i32 {
    let fixture = unsafe { &mut *ctx.cast::<Fixture>() };
    fixture.stores += 1;
    if fixture.store_status != OK {
        return fixture.store_status;
    }
    fixture.seed = Some(Zeroizing::new(unsafe { *seed.cast::<[u8; 32]>() }));
    unsafe { output.copy_from_nonoverlapping(HANDLE.as_ptr(), 36) };
    OK
}
unsafe extern "C" fn load(ctx: *mut c_void, _: *const u8, output: *mut u8) -> i32 {
    let fixture = unsafe { &mut *ctx.cast::<Fixture>() };
    fixture.loads += 1;
    if fixture.load_status != OK {
        // Simulate a provider failing after partially populating the seed buffer.
        unsafe { output.write_bytes(17, 32) };
        return fixture.load_status;
    }
    match &fixture.seed {
        Some(seed) => {
            unsafe { output.copy_from_nonoverlapping(seed.as_ptr(), 32) };
            OK
        }
        None => AUTHENTICATION,
    }
}
unsafe extern "C" fn delete(ctx: *mut c_void, _: *const u8) -> i32 {
    let fixture = unsafe { &mut *ctx.cast::<Fixture>() };
    fixture.deletes += 1;
    if fixture.delete_status != OK {
        return fixture.delete_status;
    }
    fixture.seed = None;
    OK
}
fn callbacks(fixture: &mut Fixture) -> VaultCallbacks {
    VaultCallbacks {
        abi_version: 1,
        context: (fixture as *mut Fixture).cast(),
        store: Some(save),
        load: Some(load),
        delete: Some(delete),
    }
}
fn create(cb: &VaultCallbacks) -> (i32, [u8; 36], [u8; 32]) {
    let mut handle = [255; 36];
    let mut public = [255; 32];
    let code = unsafe { links_identity_create(cb, handle.as_mut_ptr(), public.as_mut_ptr()) };
    (code, handle, public)
}
#[test]
fn ffi_create_restore_sign_and_delete() {
    let mut fixture = Fixture::default();
    let cb = callbacks(&mut fixture);
    let (code, handle, public) = create(&cb);
    assert_eq!(code, OK);
    assert_eq!(&handle, HANDLE);
    let mut restored = [0; 32];
    assert_eq!(
        unsafe { links_identity_public_key(&cb, handle.as_ptr(), restored.as_mut_ptr()) },
        OK
    );
    assert_eq!(restored, public);
    let mut signature = [0; 64];
    let message = b"links/test/v1\0test";
    assert_eq!(
        unsafe {
            links_identity_sign(
                &cb,
                handle.as_ptr(),
                public.as_ptr(),
                message.as_ptr(),
                message.len(),
                signature.as_mut_ptr(),
            )
        },
        OK
    );
    links_identity::verify(&public, message, &signature).unwrap();
    // One unwrap per operation, never cached seed state in the bridge.
    assert_eq!(fixture.loads, 3);
    assert_eq!(unsafe { links_identity_delete(&cb, handle.as_ptr()) }, OK);
    assert_eq!(unsafe { links_identity_delete(&cb, handle.as_ptr()) }, OK);
    assert_eq!(
        unsafe { links_identity_public_key(&cb, handle.as_ptr(), restored.as_mut_ptr()) },
        AUTHENTICATION
    );
    assert_eq!(restored, [0; 32]);
    assert_eq!(fixture.stores, 1);
}
#[test]
fn failures_have_no_software_fallback_or_output() {
    for code in [UNAVAILABLE, AUTHENTICATION, PROVIDER, 123] {
        let mut fixture = Fixture {
            store_status: code,
            ..Default::default()
        };
        let cb = callbacks(&mut fixture);
        let (status, handle, public) = create(&cb);
        assert_ne!(status, OK);
        assert_eq!(handle, [0; 36]);
        assert_eq!(public, [0; 32]);
        assert_eq!(fixture.loads, 0);
        assert!(fixture.seed.is_none());
    }
}
#[test]
fn failed_readback_cleans_up_new_identity() {
    let mut fixture = Fixture {
        load_status: AUTHENTICATION,
        ..Default::default()
    };
    let cb = callbacks(&mut fixture);
    assert_eq!(create(&cb), (AUTHENTICATION, [0; 36], [0; 32]));
    assert_eq!(fixture.deletes, 1);
    assert!(fixture.seed.is_none());
}
#[test]
fn substituted_seed_and_locked_vault_do_not_sign() {
    let mut fixture = Fixture::default();
    let cb = callbacks(&mut fixture);
    let (_, handle, public) = create(&cb);
    fixture.seed = Some(Zeroizing::new([9; 32]));
    let mut signature = [255; 64];
    assert_eq!(
        unsafe {
            links_identity_sign(
                &cb,
                handle.as_ptr(),
                public.as_ptr(),
                std::ptr::null(),
                0,
                signature.as_mut_ptr(),
            )
        },
        AUTHENTICATION
    );
    assert_eq!(signature, [0; 64]);
    fixture.load_status = UNAVAILABLE;
    assert_eq!(
        unsafe {
            links_identity_sign(
                &cb,
                handle.as_ptr(),
                public.as_ptr(),
                std::ptr::null(),
                0,
                signature.as_mut_ptr(),
            )
        },
        UNAVAILABLE
    );
    assert_eq!(fixture.stores, 1);
}
#[test]
fn malformed_calls_are_rejected_before_callbacks() {
    let mut fixture = Fixture::default();
    let mut cb = callbacks(&mut fixture);
    cb.abi_version = 2;
    assert_eq!(create(&cb).0, INVALID);
    cb.abi_version = 1;
    cb.load = None;
    assert_eq!(create(&cb).0, INVALID);
    cb.load = Some(load);
    let mut output = [255; 64];
    assert_eq!(
        unsafe {
            links_identity_sign(
                &cb,
                HANDLE.as_ptr(),
                [0; 32].as_ptr(),
                std::ptr::null(),
                1,
                output.as_mut_ptr(),
            )
        },
        INVALID
    );
    assert_eq!(
        unsafe {
            links_identity_sign(
                &cb,
                HANDLE.as_ptr(),
                [0; 32].as_ptr(),
                [0].as_ptr(),
                MAX_MESSAGE + 1,
                output.as_mut_ptr(),
            )
        },
        INVALID
    );
    assert_eq!(
        unsafe { links_identity_public_key(&cb, [b'A'; 36].as_ptr(), output.as_mut_ptr()) },
        INVALID
    );
    assert_eq!(
        unsafe {
            links_identity_create(std::ptr::null(), output.as_mut_ptr(), std::ptr::null_mut())
        },
        INVALID
    );
    assert_eq!(fixture.stores + fixture.loads + fixture.deletes, 0);
}
#[test]
fn empty_message_and_delete_failure_are_supported() {
    let mut fixture = Fixture::default();
    let cb = callbacks(&mut fixture);
    let (_, handle, public) = create(&cb);
    let mut signature = [0; 64];
    assert_eq!(
        unsafe {
            links_identity_sign(
                &cb,
                handle.as_ptr(),
                public.as_ptr(),
                std::ptr::null(),
                0,
                signature.as_mut_ptr(),
            )
        },
        OK
    );
    links_identity::verify(&public, b"", &signature).unwrap();
    fixture.delete_status = PROVIDER;
    assert_eq!(
        unsafe { links_identity_delete(&cb, handle.as_ptr()) },
        PROVIDER
    );
    assert!(fixture.seed.is_some());
}
#[test]
fn panics_do_not_cross_the_boundary() {
    assert_eq!(boundary(|| panic!("test panic")), PROVIDER);
}
