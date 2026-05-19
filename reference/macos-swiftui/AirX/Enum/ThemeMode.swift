//
//  ThemeMode.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-06-14.
//

import Foundation


enum ThemeMode: String, CaseIterable {
   case light = "Light Mode"
   case dark = "Dark Mode"
   
   var theme: Theme {
       switch self {
       case .light:
           return LightMode()
       case .dark:
           return DarkMode()
       }
   }
}

