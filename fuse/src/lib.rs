use anyhow::{Context, Result};
use blake_tree::StreamStorage;
use fuse::server::fuse_rpc;
use fuse::server::prelude::*;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const FS_NAME: &str = "blake-tree";

struct FuseFs {
    store: StreamStorage,
}

impl FuseFs {
    pub fn new(store: StreamStorage) -> Self {
        Self { store }
    }
}

impl<S: FuseSocket> fuse_rpc::Handlers<S> for FuseFs {
    fn opendir(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &OpendirRequest,
    ) -> fuse_rpc::SendResult<OpendirResponse, S::Error> {
        todo!()
    }

    fn readdir(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &ReaddirRequest,
    ) -> fuse_rpc::SendResult<ReaddirResponse, S::Error> {
        todo!()
    }

    fn releasedir(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &ReleasedirRequest,
    ) -> fuse_rpc::SendResult<ReleasedirResponse, S::Error> {
        todo!()
    }

    fn access(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &AccessRequest,
    ) -> fuse_rpc::SendResult<AccessResponse, S::Error> {
        todo!()
    }

    fn getattr(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &GetattrRequest,
    ) -> fuse_rpc::SendResult<GetattrResponse, S::Error> {
        todo!()
    }

    fn open(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &OpenRequest,
    ) -> fuse_rpc::SendResult<OpenResponse, S::Error> {
        todo!()
    }

    fn read(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &ReadRequest,
    ) -> fuse_rpc::SendResult<ReadResponse, S::Error> {
        todo!()
    }

    fn release(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &ReleaseRequest,
    ) -> fuse_rpc::SendResult<ReleaseResponse, S::Error> {
        todo!()
    }

    fn create(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &CreateRequest,
    ) -> fuse_rpc::SendResult<CreateResponse, S::Error> {
        todo!()
    }

    fn write(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &WriteRequest,
    ) -> fuse_rpc::SendResult<WriteResponse, S::Error> {
        todo!()
    }

    fn flush(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &FlushRequest,
    ) -> fuse_rpc::SendResult<FlushResponse, S::Error> {
        todo!()
    }

    fn unlink(
        &self,
        _call: fuse_rpc::Call<S>,
        _request: &UnlinkRequest,
    ) -> fuse_rpc::SendResult<UnlinkResponse, S::Error> {
        todo!()
    }
}

fn getuid() -> u32 {
    unsafe { libc::getuid() }
}

fn getgid() -> u32 {
    unsafe { libc::getgid() }
}

pub fn mount(mount_target: &Path) -> Result<fuse_libc::FuseServerSocket> {
    use fuse::os::linux::FuseSubtype;
    use fuse::os::linux::MountSource;

    let target_cstr = CString::new(mount_target.as_os_str().as_bytes())?;

    let fs_source = CString::new(FS_NAME)?;
    let fs_source = MountSource::new(&fs_source).unwrap();

    let fs_subtype = CString::new(FS_NAME)?;
    let fs_subtype = FuseSubtype::new(&fs_subtype).unwrap();

    let mut mount_options = fuse::os::linux::MountOptions::new();
    mount_options.set_mount_source(fs_source);
    mount_options.set_subtype(Some(fs_subtype));
    mount_options.set_user_id(Some(getuid()));
    mount_options.set_group_id(Some(getgid()));
    fuse_libc::os::linux::mount(&target_cstr, mount_options)
        .map_err(|err| {
            anyhow::anyhow!(
                "failed to mount at {}, are you running as root? {:?}",
                mount_target.display(),
                err
            )
        })
        .with_context(|| format!("mounting at {}", mount_target.display()))
}

pub fn blake_tree_fuse(store: StreamStorage, dev_fuse: fuse_libc::FuseServerSocket) -> Result<()> {
    let fs = FuseFs::new(store);
    let conn = FuseServer::new()
        .connect(dev_fuse)
        .map_err(|err| anyhow::anyhow!("failed to connect to fuse: {:?}", err))?;
    fuse_rpc::serve(&conn, &fs);
    Ok(())
}
