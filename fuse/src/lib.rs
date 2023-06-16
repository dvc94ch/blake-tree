use anyhow::{Context, Result};
use fuse::node;
use fuse::server::fuse_rpc;
use fuse::server::prelude::*;
use peershare_core::{Range, StreamId, StreamStorage};
use std::ffi::CString;
use std::io::Read;
use std::num::NonZeroU64;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

const FS_NAME: &str = "blake-tree";

struct FuseFs {
    store: StreamStorage,
    nodes: Mutex<Arc<[StreamId]>>,
}

impl FuseFs {
    pub fn new(store: StreamStorage) -> Self {
        Self {
            store,
            nodes: Mutex::new(Arc::new([])),
        }
    }

    fn nodes(&self) -> Arc<[StreamId]> {
        self.nodes.lock().unwrap().clone()
    }

    fn set_nodes(&self) {
        let nodes = self.store.streams().collect::<Vec<_>>();
        *self.nodes.lock().unwrap() = nodes.into();
    }

    fn node_id(&self, id: &StreamId) -> Option<node::Id> {
        // TODO: binary search
        self.nodes()
            .iter()
            .position(|x| x == id)
            .map(|pos| node::Id::new(pos as u64 + 2).unwrap())
    }

    fn stream_id(&self, node_id: u64) -> Option<StreamId> {
        self.nodes().get(node_id as usize - 2).copied()
    }

    fn root_attrs(&self) -> node::Attributes {
        let mut attr = node::Attributes::new(node::Id::new(1).unwrap());
        attr.set_user_id(getuid());
        attr.set_group_id(getgid());
        attr.set_mode(node::Mode::S_IFDIR | 0o777);
        attr
    }

    fn stream_attrs(&self, node_id: node::Id, stream_id: &StreamId) -> node::Attributes {
        let mut attr = node::Attributes::new(node_id);
        attr.set_user_id(getuid());
        attr.set_group_id(getgid());
        attr.set_mode(node::Mode::S_IFREG | 0o555);
        attr.set_size(stream_id.length());
        attr
    }
}

impl<S: FuseSocket> fuse_rpc::Handlers<S> for FuseFs {
    fn getattr(
        &self,
        call: fuse_rpc::Call<S>,
        request: &GetattrRequest,
    ) -> fuse_rpc::SendResult<GetattrResponse, S::Error> {
        let attr = if request.node_id().is_root() {
            self.root_attrs()
        } else if let Some(id) = self.stream_id(request.node_id().get()) {
            self.stream_attrs(request.node_id(), &id)
        } else {
            return call.respond_err(fuse::Error::NOT_FOUND);
        };
        let resp = GetattrResponse::new(attr);
        call.respond_ok(&resp)
    }

    fn opendir(
        &self,
        call: fuse_rpc::Call<S>,
        request: &OpendirRequest,
    ) -> fuse_rpc::SendResult<OpendirResponse, S::Error> {
        if !request.node_id().is_root() {
            return call.respond_err(fuse::Error::NOT_FOUND);
        }
        self.set_nodes();
        let mut resp = OpendirResponse::new();
        resp.set_handle(1);
        call.respond_ok(&resp)
    }

    fn readdir(
        &self,
        call: fuse_rpc::Call<S>,
        request: &ReaddirRequest,
    ) -> fuse_rpc::SendResult<ReaddirResponse, S::Error> {
        if request.handle() != 1 {
            return call.respond_err(fuse::Error::INVALID_ARGUMENT);
        }
        if request.offset().is_some() {
            return call.respond_ok(ReaddirResponse::EMPTY);
        }
        let nodes = self.nodes();
        let mut buf = vec![0u8; request.size()];
        let mut entries = ReaddirEntriesWriter::new(&mut buf);
        for (i, id) in nodes.iter().enumerate() {
            let name = id.to_string();
            let mut entry = ReaddirEntry::new(
                node::Id::new(i as u64 + 2).unwrap(),
                node::Name::new(&name).unwrap(),
                // node offset
                NonZeroU64::new(i as u64 + 1).unwrap(),
            );
            entry.set_file_type(node::Type::Regular);
            if entries.try_push(&entry).is_err() {
                break;
            }
        }
        let resp = ReaddirResponse::new(entries.into_entries());
        call.respond_ok(&resp)
    }

    fn releasedir(
        &self,
        call: fuse_rpc::Call<S>,
        request: &ReleasedirRequest,
    ) -> fuse_rpc::SendResult<ReleasedirResponse, S::Error> {
        if request.handle() != 1 {
            return call.respond_err(fuse::Error::INVALID_ARGUMENT);
        }
        let resp = ReleasedirResponse::new();
        call.respond_ok(&resp)
    }

    fn lookup(
        &self,
        call: fuse_rpc::Call<S>,
        request: &LookupRequest,
    ) -> fuse_rpc::SendResult<LookupResponse, S::Error> {
        if !request.parent_id().is_root() {
            return call.respond_err(fuse::Error::NOT_FOUND);
        }
        let id: StreamId = if let Ok(id) = request.name().as_str().unwrap().parse() {
            id
        } else {
            return call.respond_err(fuse::Error::INVALID_ARGUMENT);
        };
        let node_id = if let Some(node_id) = self.node_id(&id) {
            node_id
        } else {
            return call.respond_err(fuse::Error::NOT_FOUND);
        };
        let attr = self.stream_attrs(node_id, &id);
        let entry = node::Entry::new(attr);
        let resp = LookupResponse::new(Some(entry));
        call.respond_ok(&resp)
    }

    fn access(
        &self,
        call: fuse_rpc::Call<S>,
        request: &AccessRequest,
    ) -> fuse_rpc::SendResult<AccessResponse, S::Error> {
        if self.stream_id(request.node_id().get()).is_none() {
            return call.respond_err(fuse::Error::NOT_FOUND);
        }
        let resp = AccessResponse::new();
        call.respond_ok(&resp)
    }

    fn unlink(
        &self,
        call: fuse_rpc::Call<S>,
        request: &UnlinkRequest,
    ) -> fuse_rpc::SendResult<UnlinkResponse, S::Error> {
        let id: StreamId = if let Ok(id) = request.name().as_str().unwrap().parse() {
            id
        } else {
            return call.respond_err(fuse::Error::INVALID_ARGUMENT);
        };
        // TODO: return NOT_FOUND if file doesn't exist
        if let Err(err) = self.store.remove(&id) {
            log::error!("unlink: {}", err);
            return call.respond_err(fuse::Error::UNAVAILABLE);
        }
        let resp = UnlinkResponse::new();
        call.respond_ok(&resp)
    }

    fn open(
        &self,
        call: fuse_rpc::Call<S>,
        request: &OpenRequest,
    ) -> fuse_rpc::SendResult<OpenResponse, S::Error> {
        if self.stream_id(request.node_id().get()).is_none() {
            return call.respond_err(fuse::Error::NOT_FOUND);
        }
        let mut resp = OpenResponse::new();
        resp.set_handle(request.node_id().get());
        call.respond_ok(&resp)
    }

    fn read(
        &self,
        call: fuse_rpc::Call<S>,
        request: &ReadRequest,
    ) -> fuse_rpc::SendResult<ReadResponse, S::Error> {
        let id = if let Some(id) = self.stream_id(request.handle()) {
            id
        } else {
            return call.respond_err(fuse::Error::NOT_FOUND);
        };
        let range = Range::new(request.offset(), request.size() as _);
        let mut buf = Vec::with_capacity(range.length() as usize);
        if let Err(err) = (|| -> Result<()> {
            self.store
                .get(&id)?
                .read_range(range)?
                .read_to_end(&mut buf)?;
            Ok(())
        })() {
            log::error!("read: {}", err);
            return call.respond_err(fuse::Error::UNAVAILABLE);
        }
        let resp = ReadResponse::from_bytes(&buf);
        call.respond_ok(&resp)
    }

    fn release(
        &self,
        call: fuse_rpc::Call<S>,
        request: &ReleaseRequest,
    ) -> fuse_rpc::SendResult<ReleaseResponse, S::Error> {
        if self.stream_id(request.handle()).is_none() {
            return call.respond_err(fuse::Error::NOT_FOUND);
        }
        let resp = ReleaseResponse::new();
        call.respond_ok(&resp)
    }

    /*fn create(
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
    }*/
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

pub fn fuse(store: StreamStorage, dev_fuse: fuse_libc::FuseServerSocket) -> Result<()> {
    let fs = FuseFs::new(store);
    let conn = FuseServer::new()
        .connect(dev_fuse)
        .map_err(|err| anyhow::anyhow!("failed to connect to fuse: {:?}", err))?;
    fuse_rpc::serve(&conn, &fs);
    Ok(())
}
