use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, HBITMAP, HDC};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

pub struct OwnedHandle(HANDLE);

impl OwnedHandle {
    pub fn new(handle: HANDLE) -> Option<Self> {
        if handle.is_invalid() {
            None
        } else {
            Some(Self(handle))
        }
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub struct OwnedIcon(HICON);

impl OwnedIcon {
    pub fn new(icon: HICON) -> Option<Self> {
        if icon.is_invalid() {
            None
        } else {
            Some(Self(icon))
        }
    }

    pub fn raw(&self) -> HICON {
        self.0
    }
}

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

pub struct OwnedBitmap(HBITMAP);

impl OwnedBitmap {
    pub fn new(bitmap: HBITMAP) -> Option<Self> {
        if bitmap.is_invalid() {
            None
        } else {
            Some(Self(bitmap))
        }
    }

    pub fn raw(&self) -> HBITMAP {
        self.0
    }
}

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

pub struct ScreenDcGuard {
    hwnd: Option<HWND>,
    hdc: HDC,
}

impl ScreenDcGuard {
    pub fn new(hwnd: Option<HWND>, hdc: HDC) -> Option<Self> {
        if hdc.is_invalid() {
            None
        } else {
            Some(Self { hwnd, hdc })
        }
    }

    pub fn raw(&self) -> HDC {
        self.hdc
    }
}

impl Drop for ScreenDcGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ReleaseDC(self.hwnd, self.hdc);
        }
    }
}

pub struct MemoryDcGuard(HDC);

impl MemoryDcGuard {
    pub fn new(hdc: HDC) -> Option<Self> {
        if hdc.is_invalid() {
            None
        } else {
            Some(Self(hdc))
        }
    }

    pub fn raw(&self) -> HDC {
        self.0
    }
}

impl Drop for MemoryDcGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}
