#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shobjidl.h>
#include <shellapi.h>
#include <cassert>
#include <fstream>
#include <filesystem>
#include <vector>
#include "quote.h"
int wmain(int argc, wchar_t** argv) {
    assert(argc == 2);
    // Roundtrip shell argument quoting with Windows' own parser.
    for (auto value : {L"", L"C:\\a b\\", L"C:\\路径\\a'$(x).txt", L"a\\\"b"}) {
        auto line = L"program " + quoteArgument(value); int count = 0;
        auto parsed = CommandLineToArgvW(line.c_str(), &count);
        assert(parsed && count == 2 && std::wstring(parsed[1]) == value); LocalFree(parsed);
    }
    assert(SUCCEEDED(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED)));
    auto module = LoadLibraryW(argv[1]); assert(module);
    using GetFactory = HRESULT(__stdcall*)(REFCLSID, REFIID, void**);
    auto getFactory = reinterpret_cast<GetFactory>(GetProcAddress(module, "DllGetClassObject")); assert(getFactory);
    for (auto pair : {std::pair{L"{2F531697-41BE-4EC5-9402-1BBB96F238A0}", L"使用 AnyDrop 发送"}, std::pair{L"{9AB91B94-82DB-4A95-85AD-15EAC26D967B}", L"复制绝对路径"}}) {
        CLSID id; assert(SUCCEEDED(CLSIDFromString(pair.first, &id)));
        IClassFactory* factory = nullptr; assert(SUCCEEDED(getFactory(id, IID_PPV_ARGS(&factory))));
        IExplorerCommand* command = nullptr; assert(SUCCEEDED(factory->CreateInstance(nullptr, IID_PPV_ARGS(&command))));
        PWSTR title = nullptr; assert(SUCCEEDED(command->GetTitle(nullptr, &title)));
        assert(std::wstring(title) == pair.second); CoTaskMemFree(title);
        EXPCMDSTATE state; assert(SUCCEEDED(command->GetState(nullptr, FALSE, &state)) && state == ECS_HIDDEN);
        command->Release(); factory->Release();
    }
    auto unload = reinterpret_cast<HRESULT(__stdcall*)()>(GetProcAddress(module, "DllCanUnloadNow"));
    assert(unload && unload() == S_OK); FreeLibrary(module); CoUninitialize();
    return 0;
}
