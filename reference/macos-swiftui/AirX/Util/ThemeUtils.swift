//
//  LightMode.swift
//  SwiftUIPractice
//
//  Created by 刘世俊懿 on 2023-05-13.
//

import Foundation
import SwiftUI

protocol Theme {
    var gray: Color { get }
    var blue: Color { get }
    var progressTrack: Color { get }
    var progressColor: Color { get }
    var buttonText: Color { get }
    var textColor: Color { get }
}

struct LightMode: Theme {
    let gray = Color(red: 217/255, green: 217/255, blue: 217/255)
    let blue = Color(red: 111/255, green: 157/255, blue: 171/255)
    let progressTrack = Color.white.opacity(0)
    let progressColor = Color(red: 43/255, green: 87/255, blue: 101/255)
    let buttonText = Color(red: 27/255, green: 78/255, blue: 94/255)
    let textColor = Color.black
}

struct DarkMode: Theme {
    let gray = Color(red: 27/255, green: 27/255, blue: 27/255)
    let blue = Color(red: 47/255, green: 47/255, blue: 47/255) //.opacity(0.09)
    let progressTrack = Color(red: 126/255, green: 126/255, blue: 126/255)
    let progressColor = Color(red: 43/255, green: 87/255, blue: 101/255)
    let buttonText = Color(red: 27/255, green: 78/255, blue: 94/255)
    let textColor = Color(red: 188/255, green: 188/255, blue: 188/255)
}
