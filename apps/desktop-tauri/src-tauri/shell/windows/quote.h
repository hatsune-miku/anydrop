#pragma once
#include <string>
// Windows CommandLineToArgvW / C runtime quoting, including trailing backslashes.
inline std::wstring quoteArgument(const std::wstring& value) {
    std::wstring result = L"\""; size_t slashes = 0;
    for (wchar_t ch : value) {
        if (ch == L'\\') { ++slashes; continue; }
        result.append(slashes * (ch == L'"' ? 2 : 1), L'\\'); slashes = 0;
        if (ch == L'"') result += L'\\';
        result += ch;
    }
    result.append(slashes * 2, L'\\'); result += L'"'; return result;
}
