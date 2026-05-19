//
//  StringFromLengthenPointer.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-15.
//

import Foundation

extension String {
    public init?(cString: UnsafePointer<CChar>?, length: Int) {
        guard let cString else {
            self.init()
            return
        }
        let data = Data(bytes: cString, count: length)
        self.init(data: data, encoding: .utf8)
    }
}
