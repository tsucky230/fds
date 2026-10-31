//! Moving items to the platform trash (R-402-3, M1-a).
//!
//! The one rule: an item is either in the trash afterwards or exactly where it was. Nothing here
//! ever deletes permanently, even when the platform would do so when asked to recycle.

use std::io;
use std::path::Path;

#[cfg(windows)]
use std::cell::Cell;
#[cfg(windows)]
use std::rc::Rc;

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use windows::Win32::Foundation::E_ABORT;
#[cfg(windows)]
use windows::Win32::Foundation::{E_NOINTERFACE, E_POINTER, S_OK};
#[cfg(windows)]
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
#[cfg(windows)]
use windows::Win32::UI::Shell::IFileOperationProgressSink_Vtbl;
#[cfg(windows)]
use windows::Win32::UI::Shell::{
    FOF_ALLOWUNDO, FOF_NO_UI, FOFX_EARLYFAILURE, FileOperation, IFileOperation,
    IFileOperationProgressSink, IShellItem, SHCreateItemFromParsingName,
    TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
#[cfg(windows)]
use windows::core::{GUID, IUnknown, IUnknown_Vtbl, Interface};
#[cfg(windows)]
use windows::core::{HRESULT, PCWSTR};

/// Moves `path` to the trash. `path` itself is moved: a directory with its contents, or a
/// symlink / junction as the link (its target is not touched).
///
/// - Windows: the Recycle Bin through `IFileOperation` (`FOF_ALLOWUNDO`, no UI) with an
///   `IFileOperationProgressSink` whose `PreDeleteItem` returns `E_ABORT` unless the flags contain
///   `TSF_DELETE_RECYCLE_IF_POSSIBLE` (the shell would delete it permanently: a drive without a
///   Recycle Bin, a subst / network drive, an item too large for the bin, recycling turned off).
///   After `PerformOperations`, `GetAnyOperationsAborted` or a failed HRESULT -> Err. One
///   `IFileOperation` per call. COM is initialized (apartment-threaded) for the call and
///   uninitialized afterwards when this call initialized it.
/// - macOS: `trash` crate with `DeleteMethod::NsFileManager` (works without Finder / a GUI session).
/// - Other Unix: `trash` crate (freedesktop.org Trash).
///
/// Errors: `io::Error` with a readable reason (e.g. "cannot move to the Recycle Bin: ...");
/// the item is left where it was. A missing `path` is `ErrorKind::NotFound`.
pub fn move_to_trash(path: &Path) -> io::Result<()> {
    std::fs::symlink_metadata(path)?;

    #[cfg(windows)]
    {
        move_to_recycle_bin(path)
    }
    #[cfg(target_os = "macos")]
    {
        use trash::macos::TrashContextExtMacos;

        let mut ctx = trash::TrashContext::default();
        ctx.set_delete_method(trash::macos::DeleteMethod::NsFileManager);
        ctx.delete(path)
            .map_err(|error| io::Error::other(format!("cannot move to the trash: {error}")))
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        trash::delete(path)
            .map_err(|error| io::Error::other(format!("cannot move to the trash: {error}")))
    }
}

#[cfg(windows)]
struct ComApartment;

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        // CoInitializeEx succeeded on this thread, including its S_FALSE result.
        unsafe { CoUninitialize() };
    }
}

#[cfg(windows)]
#[repr(C)]
struct RecycleSink {
    vtable: *const IFileOperationProgressSink_Vtbl,
    references: Cell<u32>,
    would_delete_permanently: Rc<Cell<bool>>,
}

#[cfg(windows)]
impl RecycleSink {
    fn into_interface(would_delete_permanently: Rc<Cell<bool>>) -> IFileOperationProgressSink {
        let sink = Box::new(Self {
            vtable: &RECYCLE_SINK_VTABLE,
            references: Cell::new(1),
            would_delete_permanently,
        });
        // The boxed object starts with the sink vtable and has one owned COM reference.
        unsafe { IFileOperationProgressSink::from_raw(Box::into_raw(sink).cast()) }
    }
}

#[cfg(windows)]
unsafe extern "system" fn query_interface(
    this: *mut c_void,
    iid: *const GUID,
    interface: *mut *mut c_void,
) -> HRESULT {
    if iid.is_null() || interface.is_null() {
        return E_POINTER;
    }
    unsafe { *interface = std::ptr::null_mut() };
    if unsafe { *iid } == IFileOperationProgressSink::IID || unsafe { *iid } == IUnknown::IID {
        unsafe {
            *interface = this;
            add_ref(this);
        }
        S_OK
    } else {
        E_NOINTERFACE
    }
}

#[cfg(windows)]
unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    let sink = unsafe { &*this.cast::<RecycleSink>() };
    let references = sink.references.get().saturating_add(1);
    sink.references.set(references);
    references
}

#[cfg(windows)]
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let sink = unsafe { &*this.cast::<RecycleSink>() };
    let references = sink.references.get().saturating_sub(1);
    sink.references.set(references);
    if references == 0 {
        unsafe { drop(Box::from_raw(this.cast::<RecycleSink>())) };
    }
    references
}

#[cfg(windows)]
macro_rules! noop_sink_method {
    // `$(,)?`: newer rustfmt wraps long invocations and adds a trailing comma.
    ($name:ident ($($arg:ident: $type:ty),* $(,)?)) => {
        unsafe extern "system" fn $name(_this: *mut c_void, $($arg: $type),*) -> HRESULT {
            S_OK
        }
    };
}

#[cfg(windows)]
noop_sink_method!(start_operations());
#[cfg(windows)]
noop_sink_method!(finish_operations(_result: HRESULT));
#[cfg(windows)]
noop_sink_method!(pre_rename_item(_flags: u32, _item: *mut c_void, _name: PCWSTR));
#[cfg(windows)]
noop_sink_method!(
    post_rename_item(
        _flags: u32,
        _item: *mut c_void,
        _name: PCWSTR,
        _result: HRESULT,
        _new_item: *mut c_void,
    )
);
#[cfg(windows)]
noop_sink_method!(
    pre_move_item(_flags: u32, _item: *mut c_void, _destination: *mut c_void, _name: PCWSTR)
);
#[cfg(windows)]
noop_sink_method!(
    post_move_item(
        _flags: u32,
        _item: *mut c_void,
        _destination: *mut c_void,
        _name: PCWSTR,
        _result: HRESULT,
        _new_item: *mut c_void,
    )
);
#[cfg(windows)]
noop_sink_method!(
    pre_copy_item(_flags: u32, _item: *mut c_void, _destination: *mut c_void, _name: PCWSTR)
);
#[cfg(windows)]
noop_sink_method!(
    post_copy_item(
        _flags: u32,
        _item: *mut c_void,
        _destination: *mut c_void,
        _name: PCWSTR,
        _result: HRESULT,
        _new_item: *mut c_void,
    )
);
#[cfg(windows)]
noop_sink_method!(
    post_delete_item(_flags: u32, _item: *mut c_void, _result: HRESULT, _new_item: *mut c_void)
);
#[cfg(windows)]
noop_sink_method!(pre_new_item(_flags: u32, _destination: *mut c_void, _name: PCWSTR));
#[cfg(windows)]
noop_sink_method!(
    post_new_item(
        _flags: u32,
        _destination: *mut c_void,
        _name: PCWSTR,
        _template: PCWSTR,
        _attributes: u32,
        _result: HRESULT,
        _new_item: *mut c_void,
    )
);
#[cfg(windows)]
noop_sink_method!(update_progress(_total: u32, _done: u32));
#[cfg(windows)]
noop_sink_method!(reset_timer());
#[cfg(windows)]
noop_sink_method!(pause_timer());
#[cfg(windows)]
noop_sink_method!(resume_timer());

#[cfg(windows)]
unsafe extern "system" fn pre_delete_item(
    this: *mut c_void,
    flags: u32,
    _item: *mut c_void,
) -> HRESULT {
    if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
        let sink = unsafe { &*this.cast::<RecycleSink>() };
        sink.would_delete_permanently.set(true);
        E_ABORT
    } else {
        S_OK
    }
}

#[cfg(windows)]
static RECYCLE_SINK_VTABLE: IFileOperationProgressSink_Vtbl = IFileOperationProgressSink_Vtbl {
    base__: IUnknown_Vtbl {
        QueryInterface: query_interface,
        AddRef: add_ref,
        Release: release,
    },
    StartOperations: start_operations,
    FinishOperations: finish_operations,
    PreRenameItem: pre_rename_item,
    PostRenameItem: post_rename_item,
    PreMoveItem: pre_move_item,
    PostMoveItem: post_move_item,
    PreCopyItem: pre_copy_item,
    PostCopyItem: post_copy_item,
    PreDeleteItem: pre_delete_item,
    PostDeleteItem: post_delete_item,
    PreNewItem: pre_new_item,
    PostNewItem: post_new_item,
    UpdateProgress: update_progress,
    ResetTimer: reset_timer,
    PauseTimer: pause_timer,
    ResumeTimer: resume_timer,
};

#[cfg(windows)]
fn recycle_error(error: windows::core::Error) -> io::Error {
    io::Error::other(format!("cannot move to the Recycle Bin: {error}"))
}

#[cfg(windows)]
fn move_to_recycle_bin(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let absolute = std::path::absolute(path)?;
    let wide: Vec<u16> = absolute
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let initialized =
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    initialized.ok().map_err(recycle_error)?;
    let _apartment = ComApartment;

    let operation: IFileOperation =
        unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }.map_err(recycle_error)?;
    unsafe { operation.SetOperationFlags(FOF_ALLOWUNDO | FOF_NO_UI | FOFX_EARLYFAILURE) }
        .map_err(recycle_error)?;

    let would_delete_permanently = Rc::new(Cell::new(false));
    let sink = RecycleSink::into_interface(Rc::clone(&would_delete_permanently));
    let cookie = unsafe { operation.Advise(&sink) }.map_err(recycle_error)?;

    let result = (|| {
        let item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
            .map_err(recycle_error)?;
        unsafe { operation.DeleteItem(&item, None) }.map_err(recycle_error)?;

        let performed = unsafe { operation.PerformOperations() };
        let aborted = unsafe { operation.GetAnyOperationsAborted() };
        if would_delete_permanently.get() {
            return Err(io::Error::other(
                "cannot move to the Recycle Bin: this drive has no Recycle Bin or the item is too large",
            ));
        }
        performed.map_err(recycle_error)?;
        if aborted.map_err(recycle_error)?.as_bool() {
            return Err(io::Error::other(
                "cannot move to the Recycle Bin: operation aborted",
            ));
        }
        Ok(())
    })();

    let unadvised = unsafe { operation.Unadvise(cookie) }.map_err(recycle_error);
    result.and(unadvised)
}
