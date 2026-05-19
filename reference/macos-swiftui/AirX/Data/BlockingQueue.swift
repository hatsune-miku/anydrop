//
//  BlockingQueue.swift
//  AnyDropmac
//
//  Created by Hatsune Miku on 2023-07-14.
//

import Foundation

class BlockingQueue<T> {
    private let semaphore = DispatchSemaphore(value: 0)
    private var queue = [T]()
    
    func enqueue(item: T) {
        queue.append(item)
        semaphore.signal()
    }
    
    func dequeue() -> T {
        semaphore.wait()
        return queue.removeFirst()
    }
}
