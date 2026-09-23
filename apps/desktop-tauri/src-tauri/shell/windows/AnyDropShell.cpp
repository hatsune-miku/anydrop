#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shobjidl.h>
#include <shlwapi.h>
#include <atomic>
#include <string>
#include <vector>
#include <new>
#include "quote.h"

// Keep these IDs aligned with AppxManifest.xml and register.ps1.
const CLSID SendId = {0x2f531697,0x41be,0x4ec5,{0x94,0x02,0x1b,0xbb,0x96,0xf2,0x38,0xa0}};
const CLSID CopyId = {0x9ab91b94,0x82db,0x4a95,{0x85,0xad,0x15,0xea,0xc2,0x6d,0x96,0x7b}};
static std::atomic<long> objects{0};

static std::wstring executable() {
    DWORD size = 0;
    if (RegGetValueW(HKEY_CURRENT_USER, L"Software\\AnyDrop\\Shell", L"Executable", RRF_RT_REG_SZ, nullptr, nullptr, &size) != ERROR_SUCCESS) return {};
    std::wstring path(size / sizeof(wchar_t), L'\0');
    if (RegGetValueW(HKEY_CURRENT_USER, L"Software\\AnyDrop\\Shell", L"Executable", RRF_RT_REG_SZ, nullptr, path.data(), &size) != ERROR_SUCCESS) return {};
    path.resize(wcslen(path.c_str())); return path;
}
static HRESULT copyPaths(const std::vector<std::wstring>& paths) {
    std::wstring joined;
    for (const auto& path : paths) { if (!joined.empty()) joined += L"\r\n"; joined += path; }
    auto data = GlobalAlloc(GMEM_MOVEABLE, (joined.size() + 1) * sizeof(wchar_t));
    if (!data) return E_OUTOFMEMORY;
    auto memory = GlobalLock(data);
    if (!memory) { GlobalFree(data); return HRESULT_FROM_WIN32(GetLastError()); }
    memcpy(memory, joined.c_str(), (joined.size() + 1) * sizeof(wchar_t)); GlobalUnlock(data);
    HWND owner = CreateWindowExW(0, L"STATIC", L"AnyDrop clipboard", 0, 0, 0, 0, 0, HWND_MESSAGE, nullptr, GetModuleHandleW(nullptr), nullptr);
    if (!owner) { GlobalFree(data); return HRESULT_FROM_WIN32(GetLastError()); }
    HRESULT result = S_OK;
    if (!OpenClipboard(owner)) result = HRESULT_FROM_WIN32(GetLastError());
    else {
        if (!EmptyClipboard() || !SetClipboardData(CF_UNICODETEXT, data)) result = HRESULT_FROM_WIN32(GetLastError());
        else data = nullptr; // Clipboard owns the allocation now.
        CloseClipboard();
    }
    if (data) GlobalFree(data);
    DestroyWindow(owner); return result;
}
class Command final : public IExplorerCommand {
    std::atomic<ULONG> refs{1}; bool copy;
public:
    explicit Command(bool copyValue) : copy(copyValue) { ++objects; }
    ~Command() { --objects; }
    IFACEMETHODIMP QueryInterface(REFIID id, void** out) override {
        if (!out) return E_POINTER; *out = nullptr;
        if (id == IID_IUnknown || id == __uuidof(IExplorerCommand)) { *out = static_cast<IExplorerCommand*>(this); AddRef(); return S_OK; }
        return E_NOINTERFACE;
    }
    IFACEMETHODIMP_(ULONG) AddRef() override { return ++refs; }
    IFACEMETHODIMP_(ULONG) Release() override { auto count = --refs; if (!count) delete this; return count; }
    IFACEMETHODIMP GetTitle(IShellItemArray*, PWSTR* out) override { return SHStrDupW(copy ? L"复制绝对路径" : L"使用 AnyDrop 发送", out); }
    IFACEMETHODIMP GetIcon(IShellItemArray*, PWSTR* out) override {
        try { return SHStrDupW((executable() + L",0").c_str(), out); } catch (...) { return E_OUTOFMEMORY; }
    }
    IFACEMETHODIMP GetToolTip(IShellItemArray*, PWSTR* out) override { *out = nullptr; return E_NOTIMPL; }
    IFACEMETHODIMP GetCanonicalName(GUID* out) override { *out = copy ? CopyId : SendId; return S_OK; }
    IFACEMETHODIMP GetState(IShellItemArray* items, BOOL, EXPCMDSTATE* out) override {
        *out = ECS_HIDDEN; SFGAOF attributes = 0;
        if (items && SUCCEEDED(items->GetAttributes(SIATTRIBFLAGS_AND, SFGAO_FILESYSTEM, &attributes)) && (attributes & SFGAO_FILESYSTEM)) *out = ECS_ENABLED;
        return S_OK;
    }
    IFACEMETHODIMP GetFlags(EXPCMDFLAGS* out) override { *out = ECF_DEFAULT; return S_OK; }
    IFACEMETHODIMP EnumSubCommands(IEnumExplorerCommand** out) override { *out = nullptr; return E_NOTIMPL; }
    IFACEMETHODIMP Invoke(IShellItemArray* items, IBindCtx*) override {
        try {
            if (!items) return E_INVALIDARG;
            DWORD count = 0; HRESULT hr = items->GetCount(&count); if (FAILED(hr)) return hr;
            std::vector<std::wstring> paths;
            for (DWORD i = 0; i < count; ++i) {
                IShellItem* item = nullptr; hr = items->GetItemAt(i, &item); if (FAILED(hr)) return hr;
                PWSTR path = nullptr; hr = item->GetDisplayName(SIGDN_FILESYSPATH, &path); item->Release();
                if (FAILED(hr)) return hr;
                paths.emplace_back(path); CoTaskMemFree(path);
            }
            if (paths.empty()) return E_INVALIDARG;
            if (copy) hr = copyPaths(paths);
            else {
                auto exe = executable(); if (exe.empty()) return HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND);
                auto command = quoteArgument(exe) + L" --send-files --";
                for (const auto& path : paths) command += L" " + quoteArgument(path);
                if (command.size() >= 32767) {
                    MessageBoxW(nullptr, L"所选路径总长度超过 Windows 启动参数限制，请从 AnyDrop 窗口拖入这些文件。", L"AnyDrop", MB_OK | MB_ICONINFORMATION);
                    return HRESULT_FROM_WIN32(ERROR_BUFFER_OVERFLOW);
                }
                STARTUPINFOW startup{}; startup.cb = sizeof(startup); PROCESS_INFORMATION process{};
                if (!CreateProcessW(exe.c_str(), command.data(), nullptr, nullptr, FALSE, 0, nullptr, nullptr, &startup, &process)) hr = HRESULT_FROM_WIN32(GetLastError());
                else { CloseHandle(process.hThread); CloseHandle(process.hProcess); hr = S_OK; }
            }
            if (FAILED(hr)) {
                auto message = L"操作失败，Windows 错误码：" + std::to_wstring(HRESULT_CODE(hr));
                MessageBoxW(nullptr, message.c_str(), L"AnyDrop", MB_OK | MB_ICONERROR);
            }
            return hr;
        } catch (...) { return E_OUTOFMEMORY; }
    }
};
class Factory final : public IClassFactory {
    std::atomic<ULONG> refs{1}; bool copy;
public:
    explicit Factory(bool copyValue) : copy(copyValue) { ++objects; }
    ~Factory() { --objects; }
    IFACEMETHODIMP QueryInterface(REFIID id, void** out) override {
        if (!out) return E_POINTER; *out = nullptr;
        if (id == IID_IUnknown || id == IID_IClassFactory) { *out = static_cast<IClassFactory*>(this); AddRef(); return S_OK; }
        return E_NOINTERFACE;
    }
    IFACEMETHODIMP_(ULONG) AddRef() override { return ++refs; }
    IFACEMETHODIMP_(ULONG) Release() override { auto count = --refs; if (!count) delete this; return count; }
    IFACEMETHODIMP CreateInstance(IUnknown* outer, REFIID id, void** out) override {
        if (outer) return CLASS_E_NOAGGREGATION;
        auto command = new (std::nothrow) Command(copy); if (!command) return E_OUTOFMEMORY;
        auto hr = command->QueryInterface(id, out); command->Release(); return hr;
    }
    IFACEMETHODIMP LockServer(BOOL lock) override { if (lock) ++objects; else --objects; return S_OK; }
};
extern "C" HRESULT __stdcall DllGetClassObject(REFCLSID clsid, REFIID iid, void** out) {
    if (!out) return E_POINTER; *out = nullptr;
    if (clsid != SendId && clsid != CopyId) return CLASS_E_CLASSNOTAVAILABLE;
    auto factory = new (std::nothrow) Factory(clsid == CopyId); if (!factory) return E_OUTOFMEMORY;
    auto hr = factory->QueryInterface(iid, out); factory->Release(); return hr;
}
extern "C" HRESULT __stdcall DllCanUnloadNow() { return objects == 0 ? S_OK : S_FALSE; }
