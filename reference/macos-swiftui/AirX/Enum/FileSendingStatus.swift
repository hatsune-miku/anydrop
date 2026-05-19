//
//  FileSendingStatus.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation

enum FileSendingStatus: UInt8 {
    case requested = 1
    case rejected = 2
    case accepted = 3
    case inProgress = 4
    case cancelledBySender = 5
    case cancelledByReceiver = 6
    case completed = 7
    case error = 8
}
