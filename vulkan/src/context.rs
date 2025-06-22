mod debug;
pub mod device;
pub mod error;
mod surface;

use self::{
    device::{
        memory::MemoryProperties,
        raw::{
            allocator::{
                AllocationEntry, AllocationRequest, AllocatorIndex, AllocatorStorage, Strategy,
            },
            resources::{
                RawCollection, Resource, ResourceIndex, ResourceStorage, ResourceStorageList,
            },
        },
        Device,
    },
    error::{ResourceResult, VkError, VkResult},
    surface::Surface,
};
use ash::extensions::{ext, khr};
#[cfg(debug_assertions)]
use debug::DebugUtils;
use std::cell::RefCell;
use std::convert::Infallible;
use std::error::Error;
use std::ffi::{c_char, CStr};
use std::ops::{Deref, DerefMut};
use type_kit::{
    Contains, Create, CreateResult, Destroy, DestroyResult, DropGuard, Finalize, Initialize, Marker,
};

use ash::vk;
use winit::window::Window;

fn check_required_extension_support(
    entry: &ash::Entry,
    mut extension_names: impl Iterator<Item = &'static CStr>,
) -> VkResult<Vec<*const c_char>> {
    let supported_extensions = entry.enumerate_instance_extension_properties(None)?;
    let supported = extension_names.try_fold(Vec::new(), |mut supported, req| {
        supported_extensions
            .iter()
            .any(|sup| unsafe { CStr::from_ptr(&sup.extension_name as *const _) } == req)
            .then(|| {
                supported.push(req.as_ptr());
                supported
            })
            .ok_or(VkError::ExtensionNotSupported(req))
    })?;
    Ok(supported)
}

pub struct Instance {
    instance: ash::Instance,
    _entry: ash::Entry,
}

trait InstanceExtension: Sized {
    fn load(entry: &ash::Entry, instance: &ash::Instance) -> Self;
}

impl InstanceExtension for ext::DebugUtils {
    #[inline]
    fn load(entry: &ash::Entry, instance: &ash::Instance) -> Self {
        Self::new(entry, instance)
    }
}

impl InstanceExtension for khr::Surface {
    #[inline]
    fn load(entry: &ash::Entry, instance: &ash::Instance) -> Self {
        Self::new(entry, instance)
    }
}

impl InstanceExtension for khr::Win32Surface {
    #[inline]
    fn load(entry: &ash::Entry, instance: &ash::Instance) -> Self {
        Self::new(entry, instance)
    }
}

impl Instance {
    #[inline]
    pub(crate) fn load<E: InstanceExtension>(&self) -> E {
        E::load(&self._entry, &self.instance)
    }
}

impl Deref for Instance {
    type Target = ash::Instance;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

impl DerefMut for Instance {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.instance
    }
}

impl Create for Instance {
    type Config<'a> = ();
    type CreateError = VkError;

    fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
        let entry = unsafe { ash::Entry::load()? };
        let required_extensions = Surface::iterate_required_extensions();

        #[cfg(debug_assertions)]
        let required_extensions =
            required_extensions.chain(DebugUtils::iterate_required_extensions());

        let enabled_extension_names =
            check_required_extension_support(&entry, required_extensions)?;
        #[cfg(debug_assertions)]
        let enabled_layer_names = DebugUtils::check_required_layer_support(&entry)?;

        let application_info = vk::ApplicationInfo {
            api_version: vk::API_VERSION_1_1,
            ..Default::default()
        };

        #[cfg(debug_assertions)]
        let mut debug_messenger_info = DebugUtils::create_info();

        let create_info = {
            #[cfg(debug_assertions)]
            {
                vk::InstanceCreateInfo::builder()
                    .push_next(&mut debug_messenger_info)
                    .enabled_layer_names(&enabled_layer_names)
            }
            #[cfg(not(debug_assertions))]
            {
                vk::InstanceCreateInfo::builder()
            }
        };

        let create_info = create_info
            .application_info(&application_info)
            .enabled_extension_names(&enabled_extension_names);
        let instance = unsafe { entry.create_instance(&create_info, None)? };
        Ok(Self {
            instance,
            _entry: entry,
        })
    }
}

impl Destroy for Instance {
    type Context<'a> = ();
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            self.instance.destroy_instance(None);
        }
        Ok(())
    }
}

pub struct Context {
    allocators: Box<RefCell<DropGuard<AllocatorStorage>>>,
    storage: Box<RefCell<DropGuard<ResourceStorage>>>,
    device: DropGuard<Device>,
    surface: DropGuard<Surface>,
    #[cfg(debug_assertions)]
    debug_utils: DropGuard<DebugUtils>,
    instance: DropGuard<Instance>,
}

trait DeviceExtension: Sized {
    fn load(instance: &ash::Instance, device: &ash::Device) -> Self;
}

impl DeviceExtension for khr::Swapchain {
    #[inline]
    fn load(instance: &ash::Instance, device: &ash::Device) -> Self {
        Self::new(instance, device)
    }
}

impl Context {
    pub fn build(window: &Window) -> Result<Self, Box<dyn Error>> {
        let instance = Instance::initialize(())?;
        #[cfg(debug_assertions)]
        let debug_utils = DebugUtils::create((), &instance)?;
        let surface = Surface::create(window, &instance)?;
        let device = Device::create(&surface, &instance)?;
        let allocators = Box::new(RefCell::new(DropGuard::new(AllocatorStorage::new())));
        let storage = Box::new(RefCell::new(DropGuard::new(ResourceStorage::new())));
        Ok(Self {
            allocators,
            storage,
            device: DropGuard::new(device),
            surface: DropGuard::new(surface),
            #[cfg(debug_assertions)]
            debug_utils: DropGuard::new(debug_utils),
            instance: DropGuard::new(instance),
        })
    }

    #[inline]
    pub(crate) fn load<E: DeviceExtension>(&self) -> E {
        E::load(&self.instance, &self.device)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        let _ = self.device.wait_idle();
        let _ = self.storage.borrow_mut().destroy(&self);
        let _ = self.allocators.borrow_mut().destroy(&self);
        let _ = self.device.destroy(&self.instance);
        let _ = self.surface.destroy(&self.instance);
        #[cfg(debug_assertions)]
        let _ = self.debug_utils.destroy(&self.instance);
        let _ = self.instance.finalize();
    }
}

impl Deref for Context {
    type Target = Device;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

impl DerefMut for Context {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.device
    }
}

impl Context {
    #[inline]
    pub fn create_resource<'a, R: Resource, M: Marker>(
        &self,
        config: R::Config<'a>,
    ) -> ResourceResult<ResourceIndex<R>>
    where
        ResourceStorageList: Contains<RawCollection<R>, M>,
    {
        self.storage.borrow_mut().create_resource(&self, config)
    }

    #[inline]
    pub fn destroy_resource<'a, R: Resource, M: Marker>(
        &self,
        index: ResourceIndex<R>,
    ) -> ResourceResult<()>
    where
        ResourceStorageList: Contains<RawCollection<R>, M>,
    {
        ResourceStorage::destroy_resource(&mut *self.storage.borrow_mut(), &self, index)
    }

    #[inline]
    pub fn create_allocator<'a, S: Strategy>(
        &self,
        config: S::CreateConfig<'a>,
    ) -> ResourceResult<AllocatorIndex> {
        self.allocators
            .borrow_mut()
            .create_allocator::<S>(self, config)
    }

    #[inline]
    pub fn destroy_allocator(
        &self,
        index: AllocatorIndex,
    ) -> ResourceResult<()> {
        self.allocators
            .borrow_mut()
            .destroy_allocator(self, index)
    }

    #[inline]
    pub fn allocate(
        &self,
        index: AllocatorIndex,
        req: AllocationRequest,
    ) -> ResourceResult<AllocationEntry> {
        self.allocators.borrow_mut().allocate(self, index, req)
    }

    #[inline]
    pub fn free(
        &self,
        index: AllocationEntry,
    ) -> ResourceResult<()> {
        self.allocators.borrow_mut().free(self, index)
    }
}
