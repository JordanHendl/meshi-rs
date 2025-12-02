#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GpuOption<T> {
    /// 0 = None, non-zero = Some
    pub is_some: u32,
    pub value: T,
}

impl<T: Default> Default for GpuOption<T> {
    fn default() -> Self {
        Self {
            is_some: 0,
            value: T::default(),
        }
    }
}

impl<T> GpuOption<T> {
    #[inline]
    pub fn none(default: T) -> Self {
        Self { is_some: 0, value: default }
    }

    #[inline]
    pub fn some(value: T) -> Self {
        Self { is_some: 1, value }
    }

    #[inline]
    pub fn is_some(&self) -> bool {
        self.is_some != 0
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        self.is_some == 0
    }

    #[inline]
    pub fn as_ref(&self) -> Option<&T> {
        if self.is_some != 0 { Some(&self.value) } else { None }
    }

    #[inline]
    pub fn as_mut(&mut self) -> Option<&mut T> {
        if self.is_some != 0 { Some(&mut self.value) } else { None }
    }
}
